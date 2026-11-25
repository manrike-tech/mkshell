// Sin consola adicional en Windows en builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mkshell_desktop::run();
}
