// node/build.rs — линкер-скрипт esp-hal (как в примерах trouble/esp-rs).
fn main() {
    println!("cargo::rustc-link-arg-bins=-Tlinkall.x");
}
