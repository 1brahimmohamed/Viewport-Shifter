#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod gui;
mod layout;
mod monitors;
mod shifter;
mod target;
mod worker;

use layout::{FitMode, Mode, compute_rect};

fn cmd_list() {
    println!("Physical monitors:");
    for m in monitors::physical() {
        println!("  {m:?}");
    }
    println!("Panels (config: {} surround panels):", config::Config::load().surround_panels);
    let c = config::Config::load();
    for p in monitors::panels(c.surround_panels, c.bezel_px) {
        println!("  {p:?}");
    }
    println!("Windows:");
    for w in target::list_windows() {
        println!("  {:>10x}  {:<24} {}x{}  {}", w.hwnd, w.exe, w.width, w.height, w.title);
    }
}

/// Debug gate: `--shift <title substring> <left|right|normal>`; prints the rect Windows granted.
fn cmd_shift(title: &str, side: &str) -> anyhow::Result<()> {
    let mode = match side {
        "left" => Mode::Left,
        "right" => Mode::Right,
        _ => Mode::Normal,
    };
    let w = target::find_by_title(title).ok_or_else(|| anyhow::anyhow!("no window matching {title:?}"))?;
    let hwnd = w.handle();
    let c = config::Config::load();
    let panels = monitors::panels(c.surround_panels, c.bezel_px);
    let want = compute_rect(&panels, mode, 0.5, FitMode::Full).ok_or_else(|| anyhow::anyhow!("no monitors"))?;
    let saved = shifter::save(hwnd).ok_or_else(|| anyhow::anyhow!("cannot read window"))?;
    println!("wanted: {want:?}");
    let got = shifter::apply(hwnd, want)?;
    println!("got:    {got:?}  {}", if got == want { "OK" } else { "CLAMPED" });
    println!("Press Enter to restore...");
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    shifter::restore(hwnd, saved)
}

fn attach_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> anyhow::Result<()> {
    unsafe {
        use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--list") => {
            attach_console();
            cmd_list();
        }
        Some("--shift") if args.len() == 3 => {
            attach_console();
            cmd_shift(&args[1], &args[2])?;
        }
        Some(_) => {
            attach_console();
            println!("usage: viewport-shifter [--list | --shift <title> <left|right|normal>]");
        }
        None => {
            let cfg = config::Config::load();
            let options = eframe::NativeOptions {
                viewport: eframe::egui::ViewportBuilder::default().with_inner_size([760.0, 640.0]),
                ..Default::default()
            };
            eframe::run_native(
                "Viewport Shifter",
                options,
                Box::new(move |cc| {
                    let worker = worker::spawn(cfg.clone(), cc.egui_ctx.clone());
                    Ok(Box::new(gui::App::new(cfg, worker)))
                }),
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
    }
    Ok(())
}
