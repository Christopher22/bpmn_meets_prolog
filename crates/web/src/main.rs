//! Mounts the page.

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(bpmn_web::App);
}
