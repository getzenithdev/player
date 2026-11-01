use crate::{
    session::{self, SessionState},
    storage::StorageManager,
};
use std::sync::mpsc::{self, Sender};
use tauri::{Emitter, Manager};
enum Write {
    Checkpoint(SessionState, bool),
    Flush(SessionState, Sender<Result<(), String>>),
}
pub struct PersistenceWriter {
    sender: Sender<Write>,
}
impl PersistenceWriter {
    pub fn new(app: tauri::AppHandle) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut pending = None;
            loop {
                let mut message = match pending.take().or_else(|| receiver.recv().ok()) {
                    Some(message) => message,
                    None => break,
                };
                if let Write::Checkpoint(state, false) = &message {
                    let visit_id = state.visit.as_ref().map(|v| v.id.clone());
                    while let Ok(next) = receiver.try_recv() {
                        if let Write::Checkpoint(state, false) = &next
                            && state.visit.as_ref().map(|v| v.id.clone()) == visit_id
                        {
                            message = next;
                        } else {
                            pending = Some(next);
                            break;
                        }
                    }
                }
                let (state, save, reply) = match message {
                    Write::Checkpoint(state, save) => (state, save, None),
                    Write::Flush(state, reply) => (state, false, Some(reply)),
                };
                let result = session::checkpoint(&app.state::<StorageManager>(), state, save);
                if let Err(error) = &result {
                    let _ = app.emit("audio:persistence-error", error);
                } else {
                    let _ = app.emit("library:changed", ());
                    if save {
                        let _ = app.emit("audio:notice", "Cola guardada");
                    }
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
        });
        Self { sender }
    }
    pub fn checkpoint(&self, state: SessionState, save: bool) {
        let _ = self.sender.send(Write::Checkpoint(state, save));
    }
    pub fn flush(&self, state: SessionState) -> Result<(), String> {
        let (reply, receive) = mpsc::channel();
        self.sender
            .send(Write::Flush(state, reply))
            .map_err(|e| e.to_string())?;
        receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?
    }
}
