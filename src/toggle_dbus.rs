use cosmic::iced::{self, futures::StreamExt, subscription};
use futures_channel::mpsc;
use std::fmt::Debug;
use zbus::{dbus_interface, Connection, ConnectionBuilder};

pub fn subscription() -> iced::Subscription<Event> {
    subscription::unfold("workspaces-dbus", State::Ready, move |state| {
        start_listening(state)
    })
}

#[derive(Debug)]
enum State {
    Ready,
    Waiting(Connection, mpsc::UnboundedReceiver<Event>),
    Finished,
}

async fn start_listening(state: State) -> (Option<Event>, State) {
    match state {
        State::Ready => {
            let (tx, rx) = mpsc::unbounded();
            if let Some(conn) = ConnectionBuilder::session()
                .ok()
                .and_then(|conn| conn.name("com.system76.CosmicWorkspaces").ok())
                .and_then(|conn| {
                    conn.serve_at(
                        "/com/system76/CosmicWorkspaces",
                        CosmicWorkspacesServer { tx },
                    )
                    .ok()
                })
                .map(|conn| conn.build())
            {
                if let Ok(conn) = conn.await {
                    return (None, State::Waiting(conn, rx));
                }
            }
            (None, State::Finished)
        }
        State::Waiting(conn, mut rx) => {
            if let Some(Event::Toggle) = rx.next().await {
                (Some(Event::Toggle), State::Waiting(conn, rx))
            } else {
                (None, State::Finished)
            }
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Toggle,
}

#[derive(Debug)]
struct CosmicWorkspacesServer {
    tx: mpsc::UnboundedSender<Event>,
}

#[dbus_interface(name = "com.system76.CosmicWorkspaces")]
impl CosmicWorkspacesServer {
    async fn toggle(&self) {
        self.tx.unbounded_send(Event::Toggle).unwrap();
    }
}
