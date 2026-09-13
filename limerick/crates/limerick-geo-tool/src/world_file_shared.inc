#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorldFile {
    pub locations: Vec<LocationData>,
}
