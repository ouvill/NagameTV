pub fn jikkyo_id(channel_type: &str, service_id: u16, name: &str) -> Option<String> {
    if channel_type == "GR" {
        let id = if name.contains("ＮＨＫ総合") {
            1
        } else if name.contains("Ｅテレ") {
            2
        } else if name.contains("読売テレビ") {
            4
        } else if name.contains("ＡＢＣテレビ") {
            5
        } else if name.contains("ＭＢＳ") {
            6
        } else if name.contains("関西テレビ") {
            8
        } else if name.contains("ＫＢＳ京都") {
            14
        } else {
            return None;
        };
        return Some(format!("jk{id}"));
    }
    let id = if service_id == 102 { 101 } else { service_id };
    Some(format!("jk{id}"))
}
