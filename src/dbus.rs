struct CosmicWorkspaces;

#[zbus::interface(name = "com.system76.CosmicWorkspaces")]
impl CosmicWorkspaces {
    #[zbus(signal)]
    async fn shown(&self, _emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn hidden(&self, _emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Clone, Debug)]
pub struct Interface {
    emitter: zbus::object_server::SignalEmitter<'static>,
}

impl Interface {
    pub async fn new(conn: zbus::Connection) -> zbus::Result<Self> {
        conn.object_server()
            .at("/com/system76/CosmicWorkspaces", CosmicWorkspaces)
            .await?;
        Ok(Interface {
            emitter: zbus::object_server::SignalEmitter::new(
                &conn,
                "/com/system76/CosmicWorkspaces",
            )
            .unwrap(),
        })
    }

    pub async fn shown(&self) -> zbus::Result<()> {
        CosmicWorkspaces.shown(&self.emitter).await
    }

    pub async fn hidden(&self) -> zbus::Result<()> {
        CosmicWorkspaces.hidden(&self.emitter).await
    }
}
