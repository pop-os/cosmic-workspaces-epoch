use crate::mpsc;
use cosmic::iced::futures::executor::block_on;
use cosmic::iced::{
    self,
    futures::{future, SinkExt},
    subscription,
};
use std::fmt::Debug;
use zbus::{dbus_interface, ConnectionBuilder};

// pub fn subscription() -> iced::Subscription<Event> {
//    subscription::channel("workspaces-dbus", 64, move |sender| async {
pub fn stream() -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel(20);
    std::thread::spawn(move || {
        // let runtime = tokio::runtime::Runtime::new().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
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
                std::future::pending::<()>().await;
            }
        });
    });
    receiver
    //    })
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
        self.sender.clone().send(Event::Toggle).unwrap();
    }
}
