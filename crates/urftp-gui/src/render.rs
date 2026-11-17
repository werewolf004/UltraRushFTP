//! Choice of the graphics adapter. A graphics card first (Direct3D 12 or Vulkan), then the
//! software renderer of Windows (WARP, "Microsoft Basic Render Driver"), OpenGL last:
//! virtual machines without 3D acceleration often expose an OpenGL driver that cannot draw.
//! When the window still cannot open, the program starts again with the software renderers
//! only ([`SOFTWARE_ENV`], which a user can also set by hand): WARP, then Mesa llvmpipe
//! (OpenGL drawn by the processor) when its `opengl32.dll` is next to the program — the
//! *software rendering pack* of the releases, for Windows without Direct3D 12 such as
//! Windows Server 2016.

use std::sync::Arc;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use eframe::wgpu;

/// Set (to anything but `0`): draw with the software renderer only.
pub(crate) const SOFTWARE_ENV: &str = "ULTRARUSHFTP_SOFTWARE_RENDER";

/// The software renderer was asked for.
pub(crate) fn software_forced() -> bool {
    std::env::var_os(SOFTWARE_ENV).is_some_and(|v| v != "0")
}

/// Order of preference of an adapter (lower is better); `None`: not used.
fn rank(backend: wgpu::Backend, device: wgpu::DeviceType, software_only: bool) -> Option<u8> {
    let software = device == wgpu::DeviceType::Cpu;
    if software_only {
        return match (software, backend) {
            (true, wgpu::Backend::Dx12) => Some(0),
            (true, wgpu::Backend::Gl) => Some(1),
            _ => None,
        };
    }
    let gl = backend == wgpu::Backend::Gl;
    Some(match (gl, device) {
        (false, wgpu::DeviceType::DiscreteGpu) => 0,
        (false, wgpu::DeviceType::IntegratedGpu) => 1,
        (false, wgpu::DeviceType::VirtualGpu) => 2,
        (false, wgpu::DeviceType::Other) => 3,
        (false, wgpu::DeviceType::Cpu) => 4,
        (true, wgpu::DeviceType::Cpu) => 6,
        (true, _) => 5,
    })
}

/// Renderer options of the window.
pub(crate) fn wgpu_options() -> WgpuConfiguration {
    let software_only = software_forced();
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    if software_only {
        setup.instance_descriptor.backends = wgpu::Backends::DX12 | wgpu::Backends::GL;
    }
    setup.native_adapter_selector = Some(Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
        adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .filter_map(|a| {
                let info = a.get_info();
                rank(info.backend, info.device_type, software_only).map(|r| (r, a))
            })
            .min_by_key(|(r, _)| *r)
            .map(|(_, a)| a.clone())
            .ok_or_else(|| {
                let found: Vec<String> = adapters.iter().map(|a| a.get_info()).map(|i| format!("{} ({:?})", i.name, i.backend)).collect();
                format!(
                    "no usable graphics adapter (found: {}). Without 3D acceleration (virtual machine, Windows Server 2016),                      copy the opengl32.dll file of the software rendering pack (UltraRushFTP-…-software-render.zip,                      on the release page) next to UltraRushFTP.exe.",
                    if found.is_empty() { "none".to_string() } else { found.join(", ") }
                )
            })
    }));
    WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
}

/// Starts the program again with the software renderer. `false` when it already uses
/// it, or when it cannot be started.
pub(crate) fn restart_in_software_mode() -> bool {
    if software_forced() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe).args(std::env::args_os().skip(1)).env(SOFTWARE_ENV, "1").spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{Backend, DeviceType};

    #[test]
    fn a_graphics_card_comes_first_then_warp_then_opengl() {
        let r = |b, d| rank(b, d, false).unwrap();
        assert!(r(Backend::Dx12, DeviceType::DiscreteGpu) < r(Backend::Vulkan, DeviceType::IntegratedGpu));
        assert!(r(Backend::Vulkan, DeviceType::VirtualGpu) < r(Backend::Dx12, DeviceType::Cpu));
        assert!(r(Backend::Dx12, DeviceType::Cpu) < r(Backend::Gl, DeviceType::IntegratedGpu), "WARP before OpenGL");
        assert!(r(Backend::Gl, DeviceType::Other) < r(Backend::Gl, DeviceType::Cpu));
    }

    #[test]
    fn software_mode_keeps_warp_then_llvmpipe() {
        assert_eq!(rank(Backend::Dx12, DeviceType::Cpu, true), Some(0));
        assert_eq!(rank(Backend::Dx12, DeviceType::DiscreteGpu, true), None);
        assert_eq!(rank(Backend::Gl, DeviceType::Cpu, true), Some(1), "Mesa llvmpipe after WARP");
        assert_eq!(rank(Backend::Gl, DeviceType::IntegratedGpu, true), None);
        assert_eq!(rank(Backend::Vulkan, DeviceType::Cpu, true), None);
    }
}
