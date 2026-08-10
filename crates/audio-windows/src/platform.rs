use std::{
    collections::{HashMap, HashSet},
    mem::{ManuallyDrop, size_of},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Sender, TrySendError};
use prollyglot_audio_pipeline::{SignalActivity, normalize_interleaved};
use prollyglot_core::{
    ApplicationSource, CaptureError, CaptureEvent, CaptureSelection, CaptureSession, CaptureState,
    NativeAudioFormat, PlaybackDevice, SampleFormat, SourceId, SourceSnapshot,
};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{
            CloseHandle, E_POINTER, HANDLE, RPC_E_CHANGED_MODE, S_OK, WAIT_FAILED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
            AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
            AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
            AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IActivateAudioInterfaceAsyncOperation,
            IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
            MMDeviceEnumerator, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVE_FORMAT_PCM, WAVEFORMATEX,
            WAVEFORMATEXTENSIBLE, eConsole, eRender,
        },
        System::{
            Com::{
                BLOB, CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, STGM_READ,
                StructuredStorage::{
                    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                    PropVariantClear, PropVariantToString,
                },
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                CreateEventW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
            },
            Variant::VT_BLOB,
        },
    },
    core::{
        Error as WindowsError, GUID, HRESULT, HSTRING, IUnknown, Interface, PWSTR, Ref, implement,
    },
};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const CAPTURE_READY_TIMEOUT: Duration = Duration::from_secs(10);
const PROCESS_ACTIVATION_TIMEOUT: Duration = Duration::from_secs(5);
const CAPTURE_WAIT_MILLIS: u32 = 250;
const SILENCE_TIMEOUT: Duration = Duration::from_secs(2);
const SIGNAL_THRESHOLD: f32 = 0.000_1;
const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(1);

struct ComApartment {
    uninitialize: bool,
}

impl ComApartment {
    fn initialize() -> Result<Self, CaptureError> {
        // Core Audio is usable from an existing apartment. Only balance
        // CoUninitialize when this call successfully initialized the thread.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self { uninitialize: true })
        } else if result == RPC_E_CHANGED_MODE {
            Ok(Self {
                uninitialize: false,
            })
        } else {
            Err(hresult_error("initialize COM for Windows audio", result))
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct TaskAllocatedWaveFormat(*mut WAVEFORMATEX);

impl Drop for TaskAllocatedWaveFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast())) };
    }
}

#[derive(Clone)]
struct ProcessEntry {
    parent_id: u32,
    executable: String,
}

#[derive(Default)]
struct ProcessIndex {
    entries: HashMap<u32, ProcessEntry>,
}

impl ProcessIndex {
    fn snapshot() -> Self {
        let Ok(handle) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return Self::default();
        };
        let snapshot = OwnedHandle(handle);
        let mut native_entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if unsafe { Process32FirstW(snapshot.0, &mut native_entry) }.is_err() {
            return Self::default();
        }

        let mut entries = HashMap::new();
        loop {
            entries.insert(
                native_entry.th32ProcessID,
                ProcessEntry {
                    parent_id: native_entry.th32ParentProcessID,
                    executable: wide_buffer(&native_entry.szExeFile),
                },
            );
            if unsafe { Process32NextW(snapshot.0, &mut native_entry) }.is_err() {
                break;
            }
        }
        Self { entries }
    }

    /// Audio sessions often belong to a Chromium/Firefox/Electron child. Walk
    /// through same-executable parents so process loopback targets the actual
    /// application root and includes its descendants.
    fn capture_root(&self, process_id: u32) -> u32 {
        let Some(origin) = self.entries.get(&process_id) else {
            return process_id;
        };
        let executable = &origin.executable;
        let mut current = process_id;
        let mut visited = HashSet::new();
        while visited.insert(current) {
            let Some(entry) = self.entries.get(&current) else {
                break;
            };
            if entry.parent_id == 0 || entry.parent_id == current {
                break;
            }
            let Some(parent) = self.entries.get(&entry.parent_id) else {
                break;
            };
            if !parent.executable.eq_ignore_ascii_case(executable) {
                break;
            }
            current = entry.parent_id;
        }
        current
    }

    fn executable_name(&self, process_id: u32) -> Option<String> {
        let executable = &self.entries.get(&process_id)?.executable;
        Path::new(executable)
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

struct WindowsCaptureSession {
    selection: CaptureSelection,
    stop_requested: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), CaptureError>>>,
}

impl WindowsCaptureSession {
    fn finish(&mut self) -> Result<(), CaptureError> {
        self.stop_requested.store(true, Ordering::Release);
        let worker = self.worker.take().ok_or(CaptureError::NotRunning)?;
        worker
            .join()
            .map_err(|panic| CaptureError::Worker(panic_message(panic)))?
    }
}

impl CaptureSession for WindowsCaptureSession {
    fn selection(&self) -> &CaptureSelection {
        &self.selection
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.finish()
    }
}

impl Drop for WindowsCaptureSession {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.finish();
        }
    }
}

pub(crate) fn source_snapshot() -> Result<SourceSnapshot, CaptureError> {
    let _apartment = ComApartment::initialize()?;
    unsafe { enumerate_sources() }.map_err(|error| windows_error("enumerate audio sources", error))
}

unsafe fn enumerate_sources() -> windows::core::Result<SourceSnapshot> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let default_id = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .and_then(|device| device_id(&device))
            .ok()
    };
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
    let device_count = unsafe { collection.GetCount()? };
    let processes = ProcessIndex::snapshot();

    let mut playback_devices = Vec::with_capacity(device_count as usize);
    let mut applications = HashMap::<u32, ApplicationSource>::new();

    for index in 0..device_count {
        let Ok(device) = (unsafe { collection.Item(index) }) else {
            continue;
        };
        let Ok(id) = (unsafe { device_id(&device) }) else {
            continue;
        };
        let name = unsafe { device_name(&device) }
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Playback device {}", index + 1));
        playback_devices.push(PlaybackDevice {
            id: SourceId::new(id.clone()),
            name,
            is_default: default_id.as_deref() == Some(id.as_str()),
        });

        // One process can own sessions on several endpoints. Merge these into
        // one selectable process-tree source while retaining observed devices.
        let Ok(manager) = (unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) })
        else {
            continue;
        };
        let Ok(sessions) = (unsafe { manager.GetSessionEnumerator() }) else {
            continue;
        };
        let Ok(session_count) = (unsafe { sessions.GetCount() }) else {
            continue;
        };

        for session_index in 0..session_count {
            let Ok(control) = (unsafe { sessions.GetSession(session_index) }) else {
                continue;
            };
            if unsafe { control.GetState() }.ok() == Some(AudioSessionStateExpired) {
                continue;
            }
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(process_id) = (unsafe { control2.GetProcessId() }) else {
                continue;
            };
            if process_id == 0 || unsafe { control2.IsSystemSoundsSession() } == S_OK {
                continue;
            }

            let capture_process_id = processes.capture_root(process_id);
            let session_name = unsafe { session_display_name(&control) }.unwrap_or_default();
            let name = process_name(capture_process_id)
                .or_else(|| processes.executable_name(capture_process_id))
                .filter(|value| !value.is_empty())
                .or_else(|| (!session_name.is_empty()).then_some(session_name))
                .unwrap_or_else(|| format!("Application {capture_process_id}"));
            let entry =
                applications
                    .entry(capture_process_id)
                    .or_insert_with(|| ApplicationSource {
                        id: SourceId::new(format!("process:{capture_process_id}")),
                        name,
                        process_id: capture_process_id,
                        device_ids: Vec::new(),
                    });
            let device_id = SourceId::new(id.clone());
            if !entry.device_ids.contains(&device_id) {
                entry.device_ids.push(device_id);
            }
        }
    }

    playback_devices.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    let mut applications = applications.into_values().collect::<Vec<_>>();
    let mut application_name_counts = HashMap::<String, usize>::new();
    for application in &applications {
        *application_name_counts
            .entry(application.name.to_lowercase())
            .or_default() += 1;
    }
    for application in &mut applications {
        if application_name_counts
            .get(&application.name.to_lowercase())
            .copied()
            .unwrap_or_default()
            > 1
        {
            application.name = format!("{} ({})", application.name, application.process_id);
        }
    }
    applications.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.process_id.cmp(&right.process_id))
    });

    Ok(SourceSnapshot {
        playback_devices,
        applications,
    })
}

unsafe fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    let pointer = unsafe { device.GetId()? };
    unsafe { take_task_string(pointer) }
}

unsafe fn device_name(device: &IMMDevice) -> windows::core::Result<String> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ)? };
    let mut value = unsafe { store.GetValue(&PKEY_Device_FriendlyName)? };
    let mut buffer = [0_u16; 512];
    let result = unsafe { PropVariantToString(&value, &mut buffer) }.map(|()| wide_buffer(&buffer));
    let clear_result = unsafe { PropVariantClear(&mut value) };
    match (result, clear_result) {
        (Ok(name), Ok(())) => Ok(name),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

unsafe fn session_display_name(
    control: &windows::Win32::Media::Audio::IAudioSessionControl,
) -> windows::core::Result<String> {
    let pointer = unsafe { control.GetDisplayName()? };
    unsafe { take_task_string(pointer) }
}

unsafe fn take_task_string(pointer: PWSTR) -> windows::core::Result<String> {
    let result = if pointer.0.is_null() {
        Ok(String::new())
    } else {
        Ok(String::from_utf16_lossy(unsafe { pointer.as_wide() }))
    };
    unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
    result
}

fn wide_buffer(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn process_name(process_id: u32) -> Option<String> {
    let handle = OwnedHandle(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?
    });
    let mut path = vec![0_u16; 32_768];
    let mut length = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .ok()?;
    }
    let path = String::from_utf16_lossy(&path[..length as usize]);
    Path::new(&path)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
}

fn windows_error(context: &str, error: windows::core::Error) -> CaptureError {
    hresult_error(context, error.code())
}

fn hresult_error(context: &str, code: windows::core::HRESULT) -> CaptureError {
    CaptureError::Platform {
        context: context.into(),
        code: format!("0x{:08X}", code.0 as u32),
    }
}

pub(crate) fn start_capture(
    selection: CaptureSelection,
    events: Sender<CaptureEvent>,
) -> Result<Box<dyn CaptureSession>, CaptureError> {
    let target = match &selection {
        CaptureSelection::SystemOutput { device_id } => CaptureTarget::Endpoint(device_id.clone()),
        CaptureSelection::Application { process_id } if *process_id != 0 => {
            CaptureTarget::Process(*process_id)
        }
        CaptureSelection::Application { .. } => {
            return Err(CaptureError::SourceUnavailable(
                "process identifier must be non-zero".into(),
            ));
        }
    };
    start_wasapi_capture(selection, target, events)
}

#[derive(Clone)]
enum CaptureTarget {
    Endpoint(SourceId),
    Process(u32),
}

impl CaptureTarget {
    fn source_id(&self) -> SourceId {
        match self {
            Self::Endpoint(device_id) => device_id.clone(),
            Self::Process(process_id) => SourceId::new(format!("process:{process_id}")),
        }
    }

    fn worker_name(&self) -> &'static str {
        match self {
            Self::Endpoint(_) => "wasapi-endpoint-loopback",
            Self::Process(_) => "wasapi-process-loopback",
        }
    }
}

fn start_wasapi_capture(
    selection: CaptureSelection,
    target: CaptureTarget,
    events: Sender<CaptureEvent>,
) -> Result<Box<dyn CaptureSession>, CaptureError> {
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_for_worker = Arc::clone(&stop_requested);
    let (ready_sender, ready_receiver) = crossbeam_channel::bounded(1);
    let worker_name = target.worker_name();
    let worker = thread::Builder::new()
        .name(worker_name.into())
        .spawn(move || capture_worker(target, events, stop_for_worker, ready_sender))
        .map_err(|error| {
            CaptureError::Worker(format!("could not start capture thread: {error}"))
        })?;

    match ready_receiver.recv_timeout(CAPTURE_READY_TIMEOUT) {
        Ok(Ok(())) => Ok(Box::new(WindowsCaptureSession {
            selection,
            stop_requested,
            worker: Some(worker),
        })),
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(error)
        }
        Err(error) => {
            stop_requested.store(true, Ordering::Release);
            let _ = worker.join();
            Err(CaptureError::Worker(format!(
                "capture worker did not become ready: {error}"
            )))
        }
    }
}

fn capture_worker(
    target: CaptureTarget,
    events: Sender<CaptureEvent>,
    stop_requested: Arc<AtomicBool>,
    ready: Sender<Result<(), CaptureError>>,
) -> Result<(), CaptureError> {
    let _apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    let source_id = target.source_id();
    let stream = match unsafe { WasapiCaptureStream::open(&target) } {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };

    if let Err(error) = unsafe { stream.audio_client.Start() } {
        let _ = ready.send(Err(windows_error("start WASAPI loopback stream", error)));
        return Ok(());
    }
    if ready.send(Ok(())).is_err() {
        let _ = unsafe { stream.audio_client.Stop() };
        return Ok(());
    }
    publish_event(&events, CaptureEvent::State(CaptureState::Capturing))?;

    let capture_result = unsafe { stream.capture(&source_id, &events, &stop_requested) };
    let stop_result = unsafe { stream.audio_client.Stop() }
        .map_err(|error| windows_error("stop WASAPI loopback stream", error));

    match (capture_result, stop_result) {
        (Err(error), _) => {
            let _ = events.send_timeout(
                CaptureEvent::Error(error.to_string()),
                Duration::from_millis(500),
            );
            Err(error)
        }
        (Ok(()), Err(error)) => {
            let _ = events.send_timeout(
                CaptureEvent::Error(error.to_string()),
                Duration::from_millis(500),
            );
            Err(error)
        }
        (Ok(()), Ok(())) => {
            let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
            Ok(())
        }
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationCompletion {
    sender: Mutex<Option<Sender<windows::core::Result<IAudioClient>>>>,
    _activation_params: Arc<AUDIOCLIENT_ACTIVATION_PARAMS>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationCompletion_Impl {
    fn ActivateCompleted(
        &self,
        activate_operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let result = activate_operation.ok().and_then(|operation| {
            let mut activation_result = HRESULT(0);
            let mut activated_interface = None::<IUnknown>;
            unsafe {
                operation.GetActivateResult(&mut activation_result, &mut activated_interface)?;
            }
            activation_result.ok()?;
            activated_interface
                .ok_or_else(|| WindowsError::from(E_POINTER))?
                .cast::<IAudioClient>()
        });

        if let Ok(mut sender) = self.sender.lock()
            && let Some(sender) = sender.take()
        {
            let _ = sender.send(result);
        }
        Ok(())
    }
}

unsafe fn activate_process_audio_client(process_id: u32) -> Result<IAudioClient, CaptureError> {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    let activation_params = Arc::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    let completion_handler: IActivateAudioInterfaceCompletionHandler = ActivationCompletion {
        sender: Mutex::new(Some(sender)),
        _activation_params: Arc::clone(&activation_params),
    }
    .into();
    let activation_variant = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: Arc::as_ptr(&activation_params) as *mut u8,
                    },
                },
            }),
        },
    };

    let activation_operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&activation_variant),
            &completion_handler,
        )
    }
    .map_err(|error| windows_error("request WASAPI process-loopback activation", error))?;

    let result = match receiver.recv_timeout(PROCESS_ACTIVATION_TIMEOUT) {
        Ok(Ok(audio_client)) => Ok(audio_client),
        Ok(Err(error)) => Err(windows_error(
            "activate WASAPI process-loopback client",
            error,
        )),
        Err(error) => Err(CaptureError::Worker(format!(
            "process-loopback activation did not complete: {error}"
        ))),
    };
    drop(activation_operation);
    drop(completion_handler);
    drop(activation_variant);
    drop(activation_params);
    result
}

struct WasapiCaptureStream {
    audio_client: IAudioClient,
    capture_client: IAudioCaptureClient,
    sample_format: NativeAudioFormat,
    block_align: usize,
    audio_event: OwnedHandle,
    process_handle: Option<OwnedHandle>,
}

impl WasapiCaptureStream {
    unsafe fn open(target: &CaptureTarget) -> Result<Self, CaptureError> {
        match target {
            CaptureTarget::Endpoint(device_id) => unsafe { Self::open_endpoint(device_id) },
            CaptureTarget::Process(process_id) => unsafe { Self::open_process(*process_id) },
        }
    }

    unsafe fn open_endpoint(device_id: &SourceId) -> Result<Self, CaptureError> {
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| windows_error("create Windows audio device enumerator", error))?
        };
        let requested_id = HSTRING::from(device_id.0.as_str());
        let device = unsafe { enumerator.GetDevice(&requested_id) }.map_err(|error| {
            CaptureError::SourceUnavailable(format!("{} ({})", device_id, error.code()))
        })?;
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|error| windows_error("activate WASAPI audio client", error))?;
        let mix_format = TaskAllocatedWaveFormat(
            unsafe { audio_client.GetMixFormat() }
                .map_err(|error| windows_error("read playback-device mix format", error))?,
        );
        if mix_format.0.is_null() {
            return Err(CaptureError::InvalidFormat(
                "WASAPI returned a null mix format".into(),
            ));
        }

        let mix = unsafe { std::ptr::read_unaligned(mix_format.0) };
        let sample_format = unsafe { native_format(mix_format.0)? };
        let block_align = usize::from(mix.nBlockAlign);
        if sample_format.bytes_per_frame() != block_align {
            return Err(CaptureError::InvalidFormat(format!(
                "WASAPI block alignment {block_align} does not match decoded format {}",
                sample_format.bytes_per_frame()
            )));
        }

        unsafe {
            Self::initialize(
                audio_client,
                mix_format.0,
                sample_format,
                block_align,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                "initialize WASAPI endpoint loopback",
                None,
            )
        }
    }

    unsafe fn open_process(process_id: u32) -> Result<Self, CaptureError> {
        let process_handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process_id) }
            .ok()
            .map(OwnedHandle);
        let audio_client = unsafe { activate_process_audio_client(process_id)? };
        let sample_format = NativeAudioFormat {
            sample_rate: 48_000,
            channels: 2,
            sample_format: SampleFormat::I16,
        };
        let block_align = sample_format.bytes_per_frame();
        let wave_format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: sample_format.channels,
            nSamplesPerSec: sample_format.sample_rate,
            nAvgBytesPerSec: sample_format.sample_rate * block_align as u32,
            nBlockAlign: block_align as u16,
            wBitsPerSample: 16,
            cbSize: 0,
        };

        unsafe {
            Self::initialize(
                audio_client,
                &wave_format,
                sample_format,
                block_align,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                "initialize WASAPI process loopback",
                process_handle,
            )
        }
    }

    unsafe fn initialize(
        audio_client: IAudioClient,
        wave_format: *const WAVEFORMATEX,
        sample_format: NativeAudioFormat,
        block_align: usize,
        stream_flags: u32,
        context: &str,
        process_handle: Option<OwnedHandle>,
    ) -> Result<Self, CaptureError> {
        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                stream_flags,
                0,
                0,
                wave_format,
                None,
            )
        }
        .map_err(|error| windows_error(context, error))?;

        let audio_event = OwnedHandle(
            unsafe { CreateEventW(None, false, false, None) }
                .map_err(|error| windows_error("create WASAPI capture event", error))?,
        );
        unsafe { audio_client.SetEventHandle(audio_event.0) }
            .map_err(|error| windows_error("set WASAPI capture event", error))?;
        let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService() }
            .map_err(|error| windows_error("open WASAPI capture service", error))?;

        Ok(Self {
            audio_client,
            capture_client,
            sample_format,
            block_align,
            audio_event,
            process_handle,
        })
    }

    unsafe fn capture(
        &self,
        source_id: &SourceId,
        events: &Sender<CaptureEvent>,
        stop_requested: &AtomicBool,
    ) -> Result<(), CaptureError> {
        let started_at = Instant::now();
        let mut sequence = 0_u64;
        let mut silence_buffer = Vec::<u8>::new();
        let mut activity = SignalActivity::new(SILENCE_TIMEOUT, SIGNAL_THRESHOLD);
        let mut pending_state = None;
        let mut dropped_frames = 0_u64;
        let mut reported_dropped_frames = 0_u64;
        let mut last_drop_report = Duration::ZERO;

        while !stop_requested.load(Ordering::Acquire) {
            if unsafe { self.process_exited()? } {
                return Err(CaptureError::SourceUnavailable(format!(
                    "selected application {source_id} has exited"
                )));
            }

            let elapsed = started_at.elapsed();
            if let Some(state) = activity.tick(elapsed) {
                pending_state = Some(state);
            }
            flush_pending_state(events, &mut pending_state)?;
            report_dropped_frames(
                events,
                dropped_frames,
                &mut reported_dropped_frames,
                elapsed,
                &mut last_drop_report,
            )?;

            let wait = unsafe { WaitForSingleObject(self.audio_event.0, CAPTURE_WAIT_MILLIS) };
            if wait == WAIT_TIMEOUT {
                continue;
            }
            if wait == WAIT_FAILED {
                return Err(windows_error(
                    "wait for WASAPI capture event",
                    windows::core::Error::from_thread(),
                ));
            }
            if wait != WAIT_OBJECT_0 {
                return Err(CaptureError::Worker(format!(
                    "unexpected WASAPI wait result {}",
                    wait.0
                )));
            }

            loop {
                let packet_size = unsafe { self.capture_client.GetNextPacketSize() }
                    .map_err(|error| windows_error("query WASAPI packet size", error))?;
                if packet_size == 0 {
                    break;
                }

                let mut data = std::ptr::null_mut();
                let mut frames = 0_u32;
                let mut flags = 0_u32;
                unsafe {
                    self.capture_client
                        .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                }
                .map_err(|error| windows_error("read WASAPI capture buffer", error))?;

                let bytes_len = (frames as usize)
                    .checked_mul(self.block_align)
                    .ok_or_else(|| {
                        CaptureError::InvalidFormat("WASAPI packet byte length overflowed".into())
                    });
                let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                let discontinuity = flags & (AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0;
                let elapsed = started_at.elapsed();
                let frame_result = bytes_len.and_then(|bytes_len| {
                    let bytes = if silent {
                        silence_buffer.resize(bytes_len, 0);
                        silence_buffer.as_slice()
                    } else if data.is_null() {
                        return Err(CaptureError::Worker(
                            "WASAPI returned a null non-silent capture buffer".into(),
                        ));
                    } else {
                        unsafe { std::slice::from_raw_parts(data, bytes_len) }
                    };
                    normalize_interleaved(
                        sequence,
                        source_id.clone(),
                        elapsed.as_micros().min(u128::from(u64::MAX)) as u64,
                        self.sample_format,
                        bytes,
                        silent,
                        discontinuity,
                    )
                });
                let release_result = unsafe { self.capture_client.ReleaseBuffer(frames) }
                    .map_err(|error| windows_error("release WASAPI capture buffer", error));
                let frame = match (frame_result, release_result) {
                    (Ok(frame), Ok(())) => frame,
                    (Err(error), _) | (_, Err(error)) => return Err(error),
                };

                if let Some(state) = activity.observe(elapsed, frame.peak) {
                    pending_state = Some(state);
                }
                flush_pending_state(events, &mut pending_state)?;
                report_dropped_frames(
                    events,
                    dropped_frames,
                    &mut reported_dropped_frames,
                    elapsed,
                    &mut last_drop_report,
                )?;

                sequence = sequence.wrapping_add(1);
                if publish_event(events, CaptureEvent::Frame(frame))? == PublishOutcome::Dropped {
                    dropped_frames = dropped_frames.saturating_add(1);
                }
            }
        }

        Ok(())
    }

    unsafe fn process_exited(&self) -> Result<bool, CaptureError> {
        let Some(process_handle) = &self.process_handle else {
            return Ok(false);
        };
        let wait = unsafe { WaitForSingleObject(process_handle.0, 0) };
        if wait == WAIT_OBJECT_0 {
            Ok(true)
        } else if wait == WAIT_TIMEOUT {
            Ok(false)
        } else if wait == WAIT_FAILED {
            Err(windows_error(
                "check selected application state",
                windows::core::Error::from_thread(),
            ))
        } else {
            Err(CaptureError::Worker(format!(
                "unexpected process wait result {}",
                wait.0
            )))
        }
    }
}

unsafe fn native_format(pointer: *const WAVEFORMATEX) -> Result<NativeAudioFormat, CaptureError> {
    let format = unsafe { std::ptr::read_unaligned(pointer) };
    let format_tag = format.wFormatTag;
    let bits_per_sample = format.wBitsPerSample;
    let extension_size = format.cbSize;
    let sample_rate = format.nSamplesPerSec;
    let channels = format.nChannels;
    let sample_format = match format_tag {
        tag if tag == WAVE_FORMAT_PCM as u16 => integer_sample_format(bits_per_sample)?,
        WAVE_FORMAT_IEEE_FLOAT if bits_per_sample == 32 => SampleFormat::F32,
        WAVE_FORMAT_EXTENSIBLE => {
            if extension_size < 22 {
                return Err(CaptureError::InvalidFormat(format!(
                    "WAVE_FORMAT_EXTENSIBLE has only {} extension bytes",
                    extension_size
                )));
            }
            let extensible =
                unsafe { std::ptr::read_unaligned(pointer.cast::<WAVEFORMATEXTENSIBLE>()) };
            let sub_format = extensible.SubFormat;
            if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && bits_per_sample == 32 {
                SampleFormat::F32
            } else if sub_format == windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM
            {
                integer_sample_format(bits_per_sample)?
            } else {
                return Err(CaptureError::InvalidFormat(format!(
                    "unsupported WAVE_FORMAT_EXTENSIBLE subtype {:?}",
                    sub_format
                )));
            }
        }
        tag => {
            return Err(CaptureError::InvalidFormat(format!(
                "unsupported Windows wave format tag 0x{tag:04X}"
            )));
        }
    };

    NativeAudioFormat {
        sample_rate,
        channels,
        sample_format,
    }
    .validate()
}

fn integer_sample_format(bits_per_sample: u16) -> Result<SampleFormat, CaptureError> {
    match bits_per_sample {
        16 => Ok(SampleFormat::I16),
        24 => Ok(SampleFormat::I24),
        32 => Ok(SampleFormat::I32),
        bits => Err(CaptureError::InvalidFormat(format!(
            "unsupported PCM container size {bits} bits"
        ))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublishOutcome {
    Published,
    Dropped,
}

fn publish_event(
    events: &Sender<CaptureEvent>,
    event: CaptureEvent,
) -> Result<PublishOutcome, CaptureError> {
    match events.try_send(event) {
        Ok(()) => Ok(PublishOutcome::Published),
        Err(TrySendError::Full(_)) => Ok(PublishOutcome::Dropped),
        Err(TrySendError::Disconnected(_)) => Err(CaptureError::Worker(
            "capture event consumer disconnected".into(),
        )),
    }
}

fn flush_pending_state(
    events: &Sender<CaptureEvent>,
    pending_state: &mut Option<CaptureState>,
) -> Result<(), CaptureError> {
    let Some(state) = *pending_state else {
        return Ok(());
    };
    if publish_event(events, CaptureEvent::State(state))? == PublishOutcome::Published {
        *pending_state = None;
    }
    Ok(())
}

fn report_dropped_frames(
    events: &Sender<CaptureEvent>,
    total: u64,
    reported: &mut u64,
    elapsed: Duration,
    last_report: &mut Duration,
) -> Result<(), CaptureError> {
    if total == *reported || elapsed.saturating_sub(*last_report) < DROP_REPORT_INTERVAL {
        return Ok(());
    }
    *last_report = elapsed;
    if publish_event(events, CaptureEvent::FramesDropped { total })? == PublishOutcome::Published {
        *reported = total;
    }
    Ok(())
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "capture worker panicked".into())
}
