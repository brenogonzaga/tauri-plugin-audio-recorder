const COMMANDS: &[&str] = &[
    "start_recording",
    "stop_recording",
    "pause_recording",
    "resume_recording",
    "get_status",
    "get_devices",
    "get_channels",
    "check_permission",
    "request_permission",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
