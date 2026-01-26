use tray_icon::{Icon, TrayIconBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostQuitMessage, TranslateMessage, MSG,
};

pub fn run_tray_loop(shutdown_tx: tokio::sync::oneshot::Sender<()>) -> anyhow::Result<()> {
    let menu = Menu::new();
    let exit_item = MenuItem::new("Exit", true, None);
    menu.append(&exit_item)?;

    let icon = build_tray_icon()?;
    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("atom_node")
        .with_icon(icon)
        .build()?;

    let exit_id = exit_item.id().clone();
    let mut shutdown_tx = Some(shutdown_tx);

    loop {
        let mut msg = std::mem::MaybeUninit::<MSG>::zeroed();
        let result = unsafe { GetMessageW(msg.as_mut_ptr(), std::ptr::null_mut(), 0, 0) };
        if result == 0 {
            break;
        }
        if result == -1 {
            break;
        }

        let msg = unsafe { msg.assume_init() };
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == exit_id {
                if let Some(tx) = shutdown_tx.take() {
                    let _ = tx.send(());
                }
                unsafe {
                    PostQuitMessage(0);
                }
            }
        }
    }

    Ok(())
}

fn build_tray_icon() -> anyhow::Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE / 2) as i32;
    let radius = center - 2;

    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let dx = x - center;
            let dy = y - center;
            let dist2 = dx * dx + dy * dy;
            let idx = ((y as u32 * SIZE + x as u32) * 4) as usize;

            if dist2 <= radius * radius {
                rgba[idx] = 0;
                rgba[idx + 1] = 140;
                rgba[idx + 2] = 255;
                rgba[idx + 3] = 255;
            } else {
                rgba[idx + 3] = 0;
            }
        }
    }

    Ok(Icon::from_rgba(rgba, SIZE, SIZE)?)
}
