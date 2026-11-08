//! The old live WebKit diagnostic is deliberately unavailable after the CEF migration.
//! Use the synthetic avatar regression; live profiles must stay in the encrypted vault.
fn main() {
    eprintln!("The WebKit avatar diagnostic has been retired. Use the CEF native regression and in-app private diagnostics.");
    std::process::exit(1);
}
