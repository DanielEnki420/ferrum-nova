//! Transport für das Duell: im Browser ein WebSocket zum Relay, in Tests ein Paar im Speicher.

use std::collections::VecDeque;
#[cfg(test)]
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Incoming {
    Open,
    /// Steuermeldung des Relays (JSON)
    Text(String),
    /// Nachricht des Mitspielers
    Binary(Vec<u8>),
    Closed,
}

pub trait Link {
    fn send(&mut self, data: Vec<u8>);
    fn poll(&mut self) -> Vec<Incoming>;
}

/// Zwei verbundene Enden im Speicher (Tests). Wie das Relay meldet es Sitzplatz und Mitspieler.
#[cfg(test)]
pub struct MemLink {
    inbox: Arc<Mutex<VecDeque<Incoming>>>,
    peer: Arc<Mutex<VecDeque<Incoming>>>,
}

#[cfg(test)]
impl MemLink {
    pub fn pair() -> (MemLink, MemLink) {
        let a = Arc::new(Mutex::new(VecDeque::new()));
        let b = Arc::new(Mutex::new(VecDeque::new()));
        for (q, seat) in [(&a, 0), (&b, 1)] {
            let mut q = q.lock().unwrap();
            q.push_back(Incoming::Open);
            q.push_back(Incoming::Text(format!("{{\"t\":\"welcome\",\"seat\":{seat}}}")));
            q.push_back(Incoming::Text("{\"t\":\"peer\",\"here\":true}".into()));
        }
        (MemLink { inbox: a.clone(), peer: b.clone() }, MemLink { inbox: b, peer: a })
    }
}

#[cfg(test)]
impl Link for MemLink {
    fn send(&mut self, data: Vec<u8>) {
        self.peer.lock().unwrap().push_back(Incoming::Binary(data));
    }
    fn poll(&mut self) -> Vec<Incoming> {
        self.inbox.lock().unwrap().drain(..).collect()
    }
}

/// WebSocket zum Duell-Relay (nur im Browser)
#[cfg(target_arch = "wasm32")]
pub struct WsLink {
    ws: web_sys::WebSocket,
    inbox: std::rc::Rc<std::cell::RefCell<VecDeque<Incoming>>>,
    /// vor dem Öffnen gesendete Nachrichten
    queued: Vec<Vec<u8>>,
    _handlers: Vec<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>>,
}

#[cfg(target_arch = "wasm32")]
impl WsLink {
    pub fn open(url: &str) -> Result<WsLink, String> {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let ws = web_sys::WebSocket::new(url).map_err(|e| format!("{e:?}"))?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let inbox: Rc<RefCell<VecDeque<Incoming>>> = Rc::default();
        let mut handlers = Vec::new();
        let q = inbox.clone();
        let on_open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| q.borrow_mut().push_back(Incoming::Open));
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        handlers.push(on_open);
        let q = inbox.clone();
        let on_msg = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let Ok(e) = e.dyn_into::<web_sys::MessageEvent>() else { return };
            let data = e.data();
            if let Some(s) = data.as_string() {
                q.borrow_mut().push_back(Incoming::Text(s));
            } else if let Ok(buf) = data.dyn_into::<js_sys::ArrayBuffer>() {
                q.borrow_mut().push_back(Incoming::Binary(js_sys::Uint8Array::new(&buf).to_vec()));
            }
        });
        ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
        handlers.push(on_msg);
        let q = inbox.clone();
        let on_close = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| q.borrow_mut().push_back(Incoming::Closed));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        handlers.push(on_close);
        Ok(WsLink { ws, inbox, queued: Vec::new(), _handlers: handlers })
    }
}

#[cfg(target_arch = "wasm32")]
impl Link for WsLink {
    fn send(&mut self, data: Vec<u8>) {
        if self.ws.ready_state() != web_sys::WebSocket::OPEN {
            self.queued.push(data);
            return;
        }
        for m in self.queued.drain(..) {
            let _ = self.ws.send_with_u8_array(&m);
        }
        let _ = self.ws.send_with_u8_array(&data);
    }
    fn poll(&mut self) -> Vec<Incoming> {
        self.inbox.borrow_mut().drain(..).collect()
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WsLink {
    fn drop(&mut self) {
        let _ = self.ws.close();
    }
}
