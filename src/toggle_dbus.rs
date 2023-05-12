use cosmic::iced::{
    self,
    futures::{channel::mpsc, future, SinkExt},
    subscription,
};
use std::fmt::Debug;
use zbus::{dbus_interface, ConnectionBuilder};

pub fn subscription() -> iced::Subscription<Event> {
    subscription::channel("workspaces-dbus", 64, move |sender| async {
        if let Some(conn) = ConnectionBuilder::session()
            .ok()
            .and_then(|conn| conn.name("com.system76.CosmicWorkspaces").ok())
            .and_then(|conn| {
                conn.serve_at(
                    "/com/system76/CosmicWorkspaces",
                    CosmicWorkspacesServer { sender },
                )
                .ok()
            })
            .map(|conn| conn.build())
        {
            let _conn = conn.await;
            future::pending().await
        } else {
            future::pending().await
        }
    })
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Toggle,
}

#[derive(Debug)]
struct CosmicWorkspacesServer {
    sender: mpsc::Sender<Event>,
}

#[dbus_interface(name = "com.system76.CosmicWorkspaces")]
impl CosmicWorkspacesServer {
    async fn toggle(&self) {
        self.sender.clone().send(Event::Toggle).await.unwrap();
    }
}
