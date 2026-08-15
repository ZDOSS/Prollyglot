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
    ApplicationSource, CaptureError, CaptureEvent, CaptureRecovery, CaptureRecoveryKind,
    CaptureSelection, CaptureSession, CaptureState, NativeAudioFormat, PlaybackDevice,
    ResolvedCaptureSelection, SampleFormat, SourceId, SourceSnapshot,
};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{
            APPMODEL_ERROR_NO_APPLICATION, CloseHandle, E_POINTER, ERROR_INSUFFICIENT_BUFFER,
            ERROR_SUCCESS, HANDLE, RPC_E_CHANGED_MODE, S_OK, WAIT_FAILED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS,
            AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
            AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IActivateAudioInterfaceAsyncOperation,
            IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            IAudioRenderClient, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
            IMMDeviceEnumerator, MMDeviceEnumerator,
            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVE_FORMAT_PCM, WAVEFORMATEX,
            WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eConsole, eRender,
        },
        Storage::Packaging::Appx::{GetApplicationUserModelId, GetPackageFamilyName},
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

use crate::identity::{IdentityKind, stable_application_id};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const KSAUDIO_SPEAKER_STEREO: u32 = 0x0000_0003;
const CAPTURE_READY_TIMEOUT: Duration = Duration::from_secs(10);
const PROCESS_ACTIVATION_TIMEOUT: Duration = Duration::from_secs(5);
const CAPTURE_WAIT_MILLIS: u32 = 250;
const LOOPBACK_BUFFER_DURATION_100NS: i64 = 5 * 10_000_000;
const DEFAULT_DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const ENDPOINT_RECONNECT_INTERVAL: Duration = Duration::from_secs(3);
const APPLICATION_RECONNECT_MIN_INTERVAL: Duration = Duration::from_secs(1);
const APPLICATION_RECONNECT_MAX_INTERVAL: Duration = Duration::from_secs(5);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);
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

#[derive(Default)]
struct DiscoveredApplication {
    name: String,
    process_ids: HashSet<u32>,
    device_ids: Vec<SourceId>,
}

struct EnumeratedAudioSources {
    snapshot: SourceSnapshot,
    application_targets: HashMap<SourceId, Vec<u32>>,
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
    unsafe { enumerate_sources() }
        .map(|sources| sources.snapshot)
        .map_err(|error| windows_error("enumerate audio sources", error))
}

pub(crate) fn resolve_selection(
    selection: &CaptureSelection,
) -> Result<ResolvedCaptureSelection, CaptureError> {
    let _apartment = ComApartment::initialize()?;
    let sources = unsafe { enumerate_sources() }
        .map_err(|error| windows_error("enumerate audio sources", error))?;
    let snapshot = &sources.snapshot;
    match selection {
        CaptureSelection::SystemDefault => {
            let device = snapshot
                .playback_devices
                .iter()
                .find(|device| device.is_default)
                .ok_or_else(|| {
                    CaptureError::SourceUnavailable(
                        "Windows has no active default playback device".into(),
                    )
                })?;
            Ok(ResolvedCaptureSelection {
                selection: selection.clone(),
                source_id: SourceId::new("default-output"),
                display_name: format!("Follow system default — {}", device.name),
            })
        }
        CaptureSelection::SystemOutput { device_id } => {
            let device = snapshot
                .playback_devices
                .iter()
                .find(|device| &device.id == device_id)
                .ok_or_else(|| CaptureError::SourceUnavailable(device_id.to_string()))?;
            Ok(ResolvedCaptureSelection {
                selection: selection.clone(),
                source_id: device.id.clone(),
                display_name: device.name.clone(),
            })
        }
        CaptureSelection::Application { source_id } => {
            let application = snapshot
                .applications
                .iter()
                .find(|application| application.id == *source_id)
                .ok_or_else(|| CaptureError::SourceUnavailable(source_id.to_string()))?;
            if application.instance_count > 1 {
                return Err(CaptureError::AmbiguousSource(application.name.clone()));
            }
            Ok(ResolvedCaptureSelection {
                selection: selection.clone(),
                source_id: application.id.clone(),
                display_name: application.name.clone(),
            })
        }
    }
}

unsafe fn enumerate_sources() -> windows::core::Result<EnumeratedAudioSources> {
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
    let mut applications = HashMap::<SourceId, DiscoveredApplication>::new();

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
            let Some((source_id, process_name)) =
                application_identity(capture_process_id, &processes)
            else {
                continue;
            };
            let name = process_name
                .filter(|value| !value.is_empty())
                .or_else(|| (!session_name.is_empty()).then_some(session_name))
                .unwrap_or_else(|| "Application".into());
            let entry = applications
                .entry(source_id)
                .or_insert_with(|| DiscoveredApplication {
                    name,
                    ..DiscoveredApplication::default()
                });
            entry.process_ids.insert(capture_process_id);
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
    let mut application_targets = HashMap::with_capacity(applications.len());
    let mut application_sources = Vec::with_capacity(applications.len());
    for (source_id, application) in applications {
        let mut process_ids = application.process_ids.into_iter().collect::<Vec<_>>();
        process_ids.sort_unstable();
        application_sources.push(ApplicationSource {
            id: source_id.clone(),
            name: application.name,
            instance_count: process_ids.len() as u32,
            device_ids: application.device_ids,
        });
        application_targets.insert(source_id, process_ids);
    }
    application_sources.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.id.0.cmp(&right.id.0))
    });

    Ok(EnumeratedAudioSources {
        snapshot: SourceSnapshot {
            playback_devices,
            applications: application_sources,
        },
        application_targets,
    })
}

unsafe fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    let pointer = unsafe { device.GetId()? };
    unsafe { take_task_string(pointer) }
}

unsafe fn prime_loopback_endpoint(device: &IMMDevice) -> Result<(), CaptureError> {
    let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|error| windows_error("activate silent loopback-primer client", error))?;
    let mix_format = TaskAllocatedWaveFormat(
        unsafe { audio_client.GetMixFormat() }
            .map_err(|error| windows_error("read loopback-primer mix format", error))?,
    );
    if mix_format.0.is_null() {
        return Err(CaptureError::InvalidFormat(
            "WASAPI returned a null loopback-primer format".into(),
        ));
    }
    let mix = unsafe { std::ptr::read_unaligned(mix_format.0) };
    let block_align = usize::from(mix.nBlockAlign);
    if block_align == 0 {
        return Err(CaptureError::InvalidFormat(
            "WASAPI returned zero block alignment for loopback priming".into(),
        ));
    }

    unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            0,
            LOOPBACK_BUFFER_DURATION_100NS,
            0,
            mix_format.0,
            None,
        )
    }
    .map_err(|error| windows_error("initialize silent loopback-primer client", error))?;

    let frames = unsafe { audio_client.GetBufferSize() }
        .map_err(|error| windows_error("read loopback-primer buffer size", error))?;
    if frames == 0 {
        return Ok(());
    }
    let bytes_len = (frames as usize).checked_mul(block_align).ok_or_else(|| {
        CaptureError::InvalidFormat("loopback-primer buffer byte length overflowed".into())
    })?;
    let render_client: IAudioRenderClient = unsafe { audio_client.GetService() }
        .map_err(|error| windows_error("open silent loopback-primer render service", error))?;
    let buffer = unsafe { render_client.GetBuffer(frames) }
        .map_err(|error| windows_error("open silent loopback-primer buffer", error))?;
    if buffer.is_null() {
        let _ = unsafe { render_client.ReleaseBuffer(frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) };
        return Err(CaptureError::Worker(
            "WASAPI returned a null loopback-primer buffer".into(),
        ));
    }
    unsafe { std::ptr::write_bytes(buffer, 0, bytes_len) };
    unsafe { render_client.ReleaseBuffer(frames, 0) }
        .map_err(|error| windows_error("release silent loopback-primer buffer", error))
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

fn application_identity(
    process_id: u32,
    processes: &ProcessIndex,
) -> Option<(SourceId, Option<String>)> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .ok()
        .map(OwnedHandle);
    let path = handle.as_ref().and_then(process_path);
    let display_name = path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| processes.executable_name(process_id));
    let identity = handle
        .as_ref()
        .and_then(|handle| unsafe { process_application_user_model_id(handle.0) })
        .map(|value| (IdentityKind::ApplicationUserModel, value))
        .or_else(|| {
            handle
                .as_ref()
                .and_then(|handle| unsafe { process_package_family_name(handle.0) })
                .map(|value| (IdentityKind::PackageFamily, value))
        })
        .or_else(|| {
            path.clone()
                .map(|value| (IdentityKind::ExecutablePath, value))
        })
        .or_else(|| {
            processes
                .entries
                .get(&process_id)
                .map(|entry| (IdentityKind::ExecutableName, entry.executable.clone()))
        })?;
    Some((stable_application_id(identity.0, &identity.1), display_name))
}

fn process_path(handle: &OwnedHandle) -> Option<String> {
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
    Some(String::from_utf16_lossy(&path[..length as usize]))
}

unsafe fn process_application_user_model_id(handle: HANDLE) -> Option<String> {
    let mut length = 0_u32;
    let probe = unsafe { GetApplicationUserModelId(handle, &mut length, None) };
    if probe == APPMODEL_ERROR_NO_APPLICATION
        || (probe != ERROR_INSUFFICIENT_BUFFER && probe != ERROR_SUCCESS)
        || length == 0
    {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    let result =
        unsafe { GetApplicationUserModelId(handle, &mut length, Some(PWSTR(buffer.as_mut_ptr()))) };
    (result == ERROR_SUCCESS)
        .then(|| wide_buffer(&buffer))
        .filter(|value| !value.is_empty())
}

unsafe fn process_package_family_name(handle: HANDLE) -> Option<String> {
    let mut length = 0_u32;
    let probe = unsafe { GetPackageFamilyName(handle, &mut length, None) };
    if probe == APPMODEL_ERROR_NO_APPLICATION
        || (probe != ERROR_INSUFFICIENT_BUFFER && probe != ERROR_SUCCESS)
        || length == 0
    {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    let result =
        unsafe { GetPackageFamilyName(handle, &mut length, Some(PWSTR(buffer.as_mut_ptr()))) };
    (result == ERROR_SUCCESS)
        .then(|| wide_buffer(&buffer))
        .filter(|value| !value.is_empty())
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
        CaptureSelection::SystemDefault => CaptureTarget::DefaultEndpoint,
        CaptureSelection::SystemOutput { device_id } => CaptureTarget::Endpoint(device_id.clone()),
        CaptureSelection::Application { source_id } => CaptureTarget::Application {
            source_id: source_id.clone(),
            process_id: Some(resolve_application_process(source_id)?),
        },
    };
    start_wasapi_capture(selection, target, events)
}

#[derive(Clone)]
enum CaptureTarget {
    DefaultEndpoint,
    Endpoint(SourceId),
    Application {
        source_id: SourceId,
        process_id: Option<u32>,
    },
}

impl CaptureTarget {
    fn source_id(&self) -> SourceId {
        match self {
            Self::DefaultEndpoint => SourceId::new("default-output"),
            Self::Endpoint(device_id) => device_id.clone(),
            Self::Application { source_id, .. } => source_id.clone(),
        }
    }

    fn worker_name(&self) -> &'static str {
        match self {
            Self::DefaultEndpoint | Self::Endpoint(_) => "wasapi-endpoint-loopback",
            Self::Application { .. } => "wasapi-process-loopback",
        }
    }

    const fn is_endpoint(&self) -> bool {
        matches!(self, Self::DefaultEndpoint | Self::Endpoint(_))
    }

    const fn is_application(&self) -> bool {
        matches!(self, Self::Application { .. })
    }

    fn clear_application_process(&mut self) {
        if let Self::Application { process_id, .. } = self {
            *process_id = None;
        }
    }

    fn resolve_application_process(&mut self) -> Result<(), CaptureError> {
        let Self::Application {
            source_id,
            process_id,
        } = self
        else {
            return Ok(());
        };
        if process_id.is_none() {
            *process_id = Some(resolve_application_process(source_id)?);
        }
        Ok(())
    }
}

fn resolve_application_process(source_id: &SourceId) -> Result<u32, CaptureError> {
    let _apartment = ComApartment::initialize()?;
    let sources = unsafe { enumerate_sources() }
        .map_err(|error| windows_error("re-enumerate application audio sources", error))?;
    let process_ids = sources
        .application_targets
        .get(source_id)
        .ok_or_else(|| CaptureError::SourceUnavailable(source_id.to_string()))?;
    match process_ids.as_slice() {
        [process_id] => Ok(*process_id),
        [] => Err(CaptureError::SourceUnavailable(source_id.to_string())),
        _ => Err(CaptureError::AmbiguousSource(source_id.to_string())),
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
    mut target: CaptureTarget,
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
    let mut starting = true;
    let mut last_recovery_message = None::<String>;
    let mut application_retry_interval = APPLICATION_RECONNECT_MIN_INTERVAL;

    loop {
        if stop_requested.load(Ordering::Acquire) {
            let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
            return Ok(());
        }

        if let Err(error) = target.resolve_application_process() {
            if starting {
                let _ = ready.send(Err(error));
                return Ok(());
            }
            publish_application_recovery(
                &events,
                &error,
                application_retry_interval,
                &mut last_recovery_message,
            )?;
            if wait_for_stop(&stop_requested, application_retry_interval) {
                let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
                return Ok(());
            }
            application_retry_interval =
                (application_retry_interval * 2).min(APPLICATION_RECONNECT_MAX_INTERVAL);
            continue;
        }

        let stream = match unsafe { WasapiCaptureStream::open(&target) }.and_then(|stream| {
            unsafe { stream.audio_client.Start() }
                .map_err(|error| windows_error("start WASAPI loopback stream", error))?;
            Ok(stream)
        }) {
            Ok(stream) => stream,
            Err(error) if starting => {
                let _ = ready.send(Err(error));
                return Ok(());
            }
            Err(error) if target.is_endpoint() => {
                publish_endpoint_recovery(&events, &error.to_string(), &mut last_recovery_message)?;
                if wait_for_stop(&stop_requested, ENDPOINT_RECONNECT_INTERVAL) {
                    let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
                    return Ok(());
                }
                continue;
            }
            Err(error) if target.is_application() => {
                target.clear_application_process();
                publish_application_recovery(
                    &events,
                    &error,
                    application_retry_interval,
                    &mut last_recovery_message,
                )?;
                if wait_for_stop(&stop_requested, application_retry_interval) {
                    let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
                    return Ok(());
                }
                application_retry_interval =
                    (application_retry_interval * 2).min(APPLICATION_RECONNECT_MAX_INTERVAL);
                continue;
            }
            Err(error) => return publish_terminal_capture_error(&events, error),
        };

        if starting {
            if ready.send(Ok(())).is_err() {
                let _ = unsafe { stream.audio_client.Stop() };
                return Ok(());
            }
            starting = false;
        }
        last_recovery_message = None;
        application_retry_interval = APPLICATION_RECONNECT_MIN_INTERVAL;
        publish_event(&events, CaptureEvent::State(CaptureState::Capturing))?;

        let capture_result = unsafe { stream.capture(&source_id, &events, &stop_requested) };
        let stop_result = unsafe { stream.audio_client.Stop() }
            .map_err(|error| windows_error("stop WASAPI loopback stream", error));

        if stop_requested.load(Ordering::Acquire) {
            if let Err(error) = stop_result {
                return publish_terminal_capture_error(&events, error);
            }
            let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
            return Ok(());
        }

        let cycle_end = match (capture_result, stop_result) {
            (Ok(CaptureEnd::Reconnect(reason)), _) => CaptureCycleEnd::Reconnect(reason),
            (Err(error), _) | (_, Err(error)) => CaptureCycleEnd::Error(error),
            (Ok(CaptureEnd::StopRequested), Ok(())) => CaptureCycleEnd::Error(
                CaptureError::Worker("the capture stream stopped unexpectedly".into()),
            ),
        };

        if target.is_application() {
            let error = match cycle_end {
                CaptureCycleEnd::Error(error) => error,
                CaptureCycleEnd::Reconnect(reason) => CaptureError::SourceUnavailable(reason),
            };
            target.clear_application_process();
            publish_application_recovery(
                &events,
                &error,
                application_retry_interval,
                &mut last_recovery_message,
            )?;
            if wait_for_stop(&stop_requested, application_retry_interval) {
                let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
                return Ok(());
            }
            continue;
        }

        let recovery_reason = match cycle_end {
            CaptureCycleEnd::Reconnect(reason) => reason,
            CaptureCycleEnd::Error(error) => error.to_string(),
        };
        publish_endpoint_recovery(&events, &recovery_reason, &mut last_recovery_message)?;
        if wait_for_stop(&stop_requested, ENDPOINT_RECONNECT_INTERVAL) {
            let _ = events.try_send(CaptureEvent::State(CaptureState::Stopped));
            return Ok(());
        }
    }
}

fn publish_endpoint_recovery(
    events: &Sender<CaptureEvent>,
    reason: &str,
    last_message: &mut Option<String>,
) -> Result<(), CaptureError> {
    let message = format!(
        "{reason}. Prollyglot is waiting for the playback endpoint and will retry automatically."
    );
    if last_message.as_deref() != Some(message.as_str()) {
        let kind = if reason.contains("default playback device changed") {
            CaptureRecoveryKind::DefaultPlaybackDeviceChanged
        } else {
            CaptureRecoveryKind::PlaybackDeviceUnavailable
        };
        publish_event(
            events,
            CaptureEvent::Recovery(CaptureRecovery {
                kind,
                message: message.clone(),
                retry_after_millis: ENDPOINT_RECONNECT_INTERVAL.as_millis() as u64,
            }),
        )?;
        *last_message = Some(message);
    }
    Ok(())
}

fn publish_application_recovery(
    events: &Sender<CaptureEvent>,
    error: &CaptureError,
    retry_after: Duration,
    last_message: &mut Option<String>,
) -> Result<(), CaptureError> {
    let kind = match error {
        CaptureError::AmbiguousSource(_) => CaptureRecoveryKind::ApplicationAmbiguous,
        CaptureError::SourceUnavailable(message) if message.contains("exited") => {
            CaptureRecoveryKind::ApplicationExited
        }
        _ => CaptureRecoveryKind::ApplicationUnavailable,
    };
    let message = match kind {
        CaptureRecoveryKind::ApplicationAmbiguous => {
            "More than one running application matches the selection. Close the duplicate instance; Prollyglot will not choose one silently.".to_string()
        }
        _ => "The selected application closed or stopped producing audio. Prollyglot is waiting for the same application to return.".to_string(),
    };
    if last_message.as_deref() != Some(message.as_str()) {
        publish_event(
            events,
            CaptureEvent::Recovery(CaptureRecovery {
                kind,
                message: message.clone(),
                retry_after_millis: retry_after.as_millis() as u64,
            }),
        )?;
        *last_message = Some(message);
    }
    Ok(())
}

fn publish_terminal_capture_error(
    events: &Sender<CaptureEvent>,
    error: CaptureError,
) -> Result<(), CaptureError> {
    let _ = events.send_timeout(
        CaptureEvent::Error(error.to_string()),
        Duration::from_millis(500),
    );
    Err(error)
}

fn wait_for_stop(stop_requested: &AtomicBool, duration: Duration) -> bool {
    let started_at = Instant::now();
    while !stop_requested.load(Ordering::Acquire) && started_at.elapsed() < duration {
        thread::sleep(STOP_POLL_INTERVAL.min(duration.saturating_sub(started_at.elapsed())));
    }
    stop_requested.load(Ordering::Acquire)
}

enum CaptureEnd {
    StopRequested,
    Reconnect(String),
}

enum CaptureCycleEnd {
    Reconnect(String),
    Error(CaptureError),
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
    default_endpoint: Option<DefaultEndpointMonitor>,
}

struct DefaultEndpointMonitor {
    enumerator: IMMDeviceEnumerator,
    opened_device_id: String,
}

struct StreamInitialization {
    sample_format: NativeAudioFormat,
    block_align: usize,
    stream_flags: u32,
    context: &'static str,
    process_handle: Option<OwnedHandle>,
    default_endpoint: Option<DefaultEndpointMonitor>,
}

impl WasapiCaptureStream {
    unsafe fn open(target: &CaptureTarget) -> Result<Self, CaptureError> {
        match target {
            CaptureTarget::DefaultEndpoint => unsafe { Self::open_endpoint(None) },
            CaptureTarget::Endpoint(device_id) => unsafe { Self::open_endpoint(Some(device_id)) },
            CaptureTarget::Application {
                process_id: Some(process_id),
                ..
            } => unsafe { Self::open_process(*process_id) },
            CaptureTarget::Application {
                process_id: None, ..
            } => Err(CaptureError::SourceUnavailable(
                "selected application has not been resolved".into(),
            )),
        }
    }

    unsafe fn open_endpoint(requested_device_id: Option<&SourceId>) -> Result<Self, CaptureError> {
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| windows_error("create Windows audio device enumerator", error))?
        };
        let device = match requested_device_id {
            Some(device_id) => {
                let requested_id = HSTRING::from(device_id.0.as_str());
                unsafe { enumerator.GetDevice(&requested_id) }.map_err(|error| {
                    CaptureError::SourceUnavailable(format!("{} ({})", device_id, error.code()))
                })?
            }
            None => unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(
                |error| {
                    CaptureError::SourceUnavailable(format!(
                        "default playback device ({})",
                        error.code()
                    ))
                },
            )?,
        };
        let opened_device_id = unsafe { device_id(&device) }
            .map_err(|error| windows_error("read selected playback-device identifier", error))?;
        let default_endpoint = requested_device_id
            .is_none()
            .then_some(DefaultEndpointMonitor {
                enumerator,
                opened_device_id,
            });
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

        // OBS primes endpoint loopback with a silent render buffer so the
        // shared stream does not stall or develop timestamp glitches across
        // long silent periods. This is best-effort because capture must still
        // work on endpoints that reject an auxiliary render client.
        let _ = unsafe { prime_loopback_endpoint(&device) };

        unsafe {
            Self::initialize(
                audio_client,
                mix_format.0,
                StreamInitialization {
                    sample_format,
                    block_align,
                    stream_flags: AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    context: "initialize WASAPI endpoint loopback",
                    process_handle: None,
                    default_endpoint,
                },
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
            sample_format: SampleFormat::F32,
        };
        let block_align = sample_format.bytes_per_frame();
        let wave_format = WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE,
                nChannels: sample_format.channels,
                nSamplesPerSec: sample_format.sample_rate,
                nAvgBytesPerSec: sample_format.sample_rate * block_align as u32,
                nBlockAlign: block_align as u16,
                wBitsPerSample: 32,
                cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: 32,
            },
            dwChannelMask: KSAUDIO_SPEAKER_STEREO,
            SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        };

        unsafe {
            Self::initialize(
                audio_client,
                &wave_format.Format,
                StreamInitialization {
                    sample_format,
                    block_align,
                    stream_flags: AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    context: "initialize WASAPI process loopback",
                    process_handle,
                    default_endpoint: None,
                },
            )
        }
    }

    unsafe fn initialize(
        audio_client: IAudioClient,
        wave_format: *const WAVEFORMATEX,
        initialization: StreamInitialization,
    ) -> Result<Self, CaptureError> {
        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                initialization.stream_flags,
                LOOPBACK_BUFFER_DURATION_100NS,
                0,
                wave_format,
                None,
            )
        }
        .map_err(|error| windows_error(initialization.context, error))?;

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
            sample_format: initialization.sample_format,
            block_align: initialization.block_align,
            audio_event,
            process_handle: initialization.process_handle,
            default_endpoint: initialization.default_endpoint,
        })
    }

    unsafe fn capture(
        &self,
        source_id: &SourceId,
        events: &Sender<CaptureEvent>,
        stop_requested: &AtomicBool,
    ) -> Result<CaptureEnd, CaptureError> {
        let started_at = Instant::now();
        let mut sequence = 0_u64;
        let mut silence_buffer = Vec::<u8>::new();
        let mut activity = SignalActivity::new(SILENCE_TIMEOUT, SIGNAL_THRESHOLD);
        let mut pending_state = None;
        let mut dropped_frames = 0_u64;
        let mut reported_dropped_frames = 0_u64;
        let mut last_drop_report = Duration::ZERO;
        let mut last_default_device_poll = Duration::ZERO;

        while !stop_requested.load(Ordering::Acquire) {
            if unsafe { self.process_exited()? } {
                return Err(CaptureError::SourceUnavailable(format!(
                    "selected application {source_id} has exited"
                )));
            }

            let elapsed = started_at.elapsed();
            if elapsed.saturating_sub(last_default_device_poll) >= DEFAULT_DEVICE_POLL_INTERVAL {
                last_default_device_poll = elapsed;
                if unsafe { self.default_endpoint_changed()? } {
                    return Ok(CaptureEnd::Reconnect(
                        "the system default playback device changed".into(),
                    ));
                }
            }
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

        Ok(CaptureEnd::StopRequested)
    }

    unsafe fn default_endpoint_changed(&self) -> Result<bool, CaptureError> {
        let Some(default_endpoint) = &self.default_endpoint else {
            return Ok(false);
        };
        let current = unsafe {
            default_endpoint
                .enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
        }
        .map_err(|error| {
            CaptureError::SourceUnavailable(format!("default playback device ({})", error.code()))
        })?;
        let current_id = unsafe { device_id(&current) }
            .map_err(|error| windows_error("read default playback-device identifier", error))?;
        Ok(current_id != default_endpoint.opened_device_id)
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
