// node/build.rs — сборка ESP-IDF через embuild (нативный билдер esp-idf-svc).
fn main() {
    embuild::espidf::sysenv::emit();
}
