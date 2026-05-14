use serde::Serialize;

const BRANDING_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/branding.json"));

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BrandingDto {
    pub internal_id: String,
    pub display_name: String,
    pub window_title: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub heading_font: String,
    pub body_font: String,
    pub discord: String,
    pub website: String,
    pub server_name: String,
    pub server_address: String,
}

#[tauri::command]
pub fn get_branding() -> BrandingDto {
    serde_json::from_str(BRANDING_JSON).expect("branding.json malformed — rebuild")
}
