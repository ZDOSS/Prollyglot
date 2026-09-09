use std::{
    collections::HashMap,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use zbus::{
    Connection,
    message::Header,
    zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value},
};

pub mod source;

type Dict = HashMap<String, OwnedValue>;
const DEST: &str = "org.freedesktop.portal.Desktop";
const ROOT: &str = "/org/freedesktop/portal/desktop";

pub fn private_session() -> PathBuf {
    let root = PathBuf::from(
        std::env::var("PROLLYGLOT_PRIVATE_PIPEWIRE")
            .expect("Run through scripts/check-screen-capture.sh; never use the desktop portal."),
    );
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("prollyglot-pipewire-")
    );
    assert_eq!(
        std::env::var("PIPEWIRE_RUNTIME_DIR").unwrap(),
        root.to_str().unwrap()
    );
    assert_eq!(
        std::env::var("XDG_RUNTIME_DIR").unwrap(),
        root.to_str().unwrap()
    );
    assert_eq!(
        std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .unwrap()
            .split(",guid=")
            .next()
            .unwrap(),
        format!("unix:path={}/bus", root.display())
    );
    assert!(std::env::var_os("DISPLAY").is_none());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    root
}

pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn string(value: &str) -> OwnedValue {
    Value::from(value).try_to_owned().unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    Normal,
    Stall(&'static str),
    StallReply(&'static str),
    Cancel,
    Reject,
    Revoke,
    MissingSerial,
    WrongSource,
    Multiple,
}

pub struct Observed {
    pub parent_window: Mutex<String>,
    pub requests_closed: AtomicUsize,
    pub sessions_closed: AtomicUsize,
    pub methods: Mutex<Vec<String>>,
    pub session: Mutex<String>,
    pub selection: Mutex<Option<Dict>>,
    pub remote_opened: AtomicBool,
}

struct Request {
    observed: Arc<Observed>,
}
#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl Request {
    async fn close(&self) {
        self.observed.requests_closed.fetch_add(1, Ordering::AcqRel);
    }
}

struct Session {
    observed: Arc<Observed>,
}
#[zbus::interface(name = "org.freedesktop.portal.Session")]
impl Session {
    async fn close(&self) {
        self.observed.sessions_closed.fetch_add(1, Ordering::AcqRel);
    }
}

struct Portal {
    observed: Arc<Observed>,
    root: PathBuf,
    node: u32,
    serial: u64,
    version: u32,
    behavior: Behavior,
}

impl Portal {
    async fn response(
        &self,
        conn: &Connection,
        header: &Header<'_>,
        method: &str,
        options: Dict,
        code: u32,
        results: Dict,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = <&str>::try_from(options.get("handle_token").unwrap()).unwrap();
        let sender = header
            .sender()
            .unwrap()
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = OwnedObjectPath::try_from(format!("{ROOT}/request/{sender}/{token}")).unwrap();
        conn.object_server()
            .at(
                path.clone(),
                Request {
                    observed: self.observed.clone(),
                },
            )
            .await
            .unwrap();
        self.observed.methods.lock().unwrap().push(method.into());
        if matches!(self.behavior, Behavior::StallReply(stalled) if stalled == method) {
            std::future::pending::<()>().await;
        }
        if !matches!(self.behavior, Behavior::Stall(stalled) if stalled == method) {
            // Deliberately emit before returning the method reply. This is a
            // valid portal response ordering and catches missed subscriptions.
            conn.emit_signal(
                header.sender().map(|name| name.as_str()),
                &path,
                "org.freedesktop.portal.Request",
                "Response",
                &(code, results),
            )
            .await
            .unwrap();
        }
        Ok(path)
    }
}

#[zbus::interface(name = "org.freedesktop.portal.ScreenCast")]
impl Portal {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.version
    }
    #[zbus(property)]
    fn available_source_types(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn available_cursor_modes(&self) -> u32 {
        7
    }

    async fn create_session(
        &self,
        options: Dict,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = <&str>::try_from(options.get("session_handle_token").unwrap()).unwrap();
        let sender = header
            .sender()
            .unwrap()
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!("{ROOT}/session/{sender}/{token}");
        conn.object_server()
            .at(
                path.clone(),
                Session {
                    observed: self.observed.clone(),
                },
            )
            .await
            .unwrap();
        *self.observed.session.lock().unwrap() = path.clone();
        self.response(
            conn,
            &header,
            "CreateSession",
            options,
            0,
            Dict::from([("session_handle".into(), string(&path))]),
        )
        .await
    }

    async fn select_sources(
        &self,
        session: OwnedObjectPath,
        options: Dict,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(
            session.as_str(),
            self.observed.session.lock().unwrap().as_str()
        );
        *self.observed.selection.lock().unwrap() = Some(
            options
                .iter()
                .map(|(k, v)| (k.clone(), v.try_clone().unwrap()))
                .collect(),
        );
        self.response(conn, &header, "SelectSources", options, 0, Dict::new())
            .await
    }

    async fn start(
        &self,
        session: OwnedObjectPath,
        parent_window: String,
        options: Dict,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(
            session.as_str(),
            self.observed.session.lock().unwrap().as_str()
        );
        *self.observed.parent_window.lock().unwrap() = parent_window;
        if self.behavior == Behavior::Revoke {
            conn.emit_signal(
                header.sender().map(|name| name.as_str()),
                &session,
                "org.freedesktop.portal.Session",
                "Closed",
                &(Dict::new(),),
            )
            .await
            .unwrap();
        }
        let source_type = if self.behavior == Behavior::WrongSource {
            4u32
        } else {
            u32::try_from(
                self.observed
                    .selection
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .get("types")
                    .unwrap(),
            )
            .unwrap()
        };
        let mut props = Dict::from([
            (
                "position".into(),
                Value::from((-1600i32, 120i32)).try_to_owned().unwrap(),
            ),
            (
                "size".into(),
                Value::from((1600i32, 900i32)).try_to_owned().unwrap(),
            ),
            ("source_type".into(), source_type.into()),
        ]);
        if self.version >= 6 && self.behavior != Behavior::MissingSerial {
            props.insert("pipewire-serial".into(), self.serial.into());
        }
        let mut streams = vec![(self.node, props)];
        if self.behavior == Behavior::Multiple {
            streams.push((self.node, Dict::new()));
        }
        let results = Dict::from([(
            "streams".into(),
            Value::from(streams).try_to_owned().unwrap(),
        )]);
        let code = match self.behavior {
            Behavior::Cancel => 1,
            Behavior::Reject => 2,
            _ => 0,
        };
        self.response(conn, &header, "Start", options, code, results)
            .await
    }

    async fn open_pipe_wire_remote(
        &self,
        session: OwnedObjectPath,
        _options: Dict,
    ) -> zbus::fdo::Result<OwnedFd> {
        assert_eq!(
            session.as_str(),
            self.observed.session.lock().unwrap().as_str()
        );
        self.observed.remote_opened.store(true, Ordering::Release);
        Ok(
            std::os::fd::OwnedFd::from(UnixStream::connect(self.root.join("pipewire-0")).unwrap())
                .into(),
        )
    }
}

pub struct MockPortal {
    pub observed: Arc<Observed>,
    connection: Connection,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MockPortal {
    pub fn start(node: u32, serial: u64, version: u32, behavior: Behavior) -> Self {
        let root = private_session();
        let observed = Arc::new(Observed {
            parent_window: Mutex::default(),
            requests_closed: AtomicUsize::new(0),
            sessions_closed: AtomicUsize::new(0),
            methods: Mutex::default(),
            session: Mutex::default(),
            selection: Mutex::default(),
            remote_opened: AtomicBool::new(false),
        });
        let portal = Portal {
            root,
            observed: observed.clone(),
            node,
            serial,
            version,
            behavior,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let worker = thread::spawn(move || {
            block_on(async move {
                let connection = zbus::connection::Builder::session()
                    .unwrap()
                    .name(DEST)
                    .unwrap()
                    .serve_at(ROOT, portal)
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
                ready_tx.send(connection.clone()).unwrap();
                while !worker_stop.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                connection.close().await.unwrap();
            })
        });
        let connection = ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Self {
            observed,
            connection,
            stop,
            worker: Some(worker),
        }
    }

    pub fn wait_method(&self, method: &str) {
        let start = std::time::Instant::now();
        while !self
            .observed
            .methods
            .lock()
            .unwrap()
            .iter()
            .any(|m| m == method)
        {
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "portal never reached {method}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn revoke(&self) {
        let session = self.observed.session.lock().unwrap().clone();
        block_on(self.connection.emit_signal(
            None::<&str>,
            session,
            "org.freedesktop.portal.Session",
            "Closed",
            &(Dict::new(),),
        ))
        .unwrap();
    }
}

impl Drop for MockPortal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
