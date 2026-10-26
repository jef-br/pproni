//! The graphics adapters in this machine, best first.
//!
//! DirectML addresses a card by its position in DXGI's adapter list, so an [`Adapter`]'s
//! `device_id` is simply that position. "Best" means most dedicated video memory: a real card
//! (NVIDIA, AMD) carries gigabytes of its own, a shared-memory chip (Intel Iris) a token 128MB and
//! borrows system RAM for the rest. The software adapter ("Microsoft Basic Render Driver") is
//! left out — it is the CPU in disguise and slower than the CPU path.

#[derive(Debug, Clone)]
pub struct Adapter {
    /// Position in DXGI's list — the id DirectML takes.
    pub device_id: i32,
    pub name: String,
    pub dedicated_vram: usize,
}

/// Hardware adapters, most dedicated video memory first. Empty when there are none, or when
/// DXGI cannot be reached (non-Windows host): the caller then simply has no GPU to try.
pub fn ranked() -> Vec<Adapter> {
    #[cfg(windows)]
    {
        let mut list = win::enumerate();
        list.sort_by(|a, b| b.dedicated_vram.cmp(&a.dedicated_vram));
        list
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
mod win {
    use super::Adapter;
    use std::ffi::c_void;
    use windows_sys::core::GUID;

    // windows-sys 0.59 carries no DXGI bindings, so the one function and one struct needed are
    // declared here. `raw-dylib` links against dxgi.dll without needing the SDK's import library.
    #[link(name = "dxgi", kind = "raw-dylib")]
    extern "system" {
        fn CreateDXGIFactory1(riid: *const GUID, factory: *mut *mut c_void) -> i32;
    }

    /// `DXGI_ADAPTER_DESC1`.
    #[repr(C)]
    #[allow(non_snake_case)]
    struct DXGI_ADAPTER_DESC1 {
        Description: [u16; 128],
        VendorId: u32,
        DeviceId: u32,
        SubSysId: u32,
        Revision: u32,
        DedicatedVideoMemory: usize,
        DedicatedSystemMemory: usize,
        SharedSystemMemory: usize,
        AdapterLuid: [u32; 2], // LUID: low part, high part
        Flags: u32,
    }

    /// IID of `IDXGIFactory1`.
    const IID_IDXGIFACTORY1: GUID = GUID {
        data1: 0x770a_ae78,
        data2: 0xf26f,
        data3: 0x4dba,
        data4: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
    };
    /// `DXGI_ADAPTER_FLAG_SOFTWARE`.
    const FLAG_SOFTWARE: u32 = 2;
    /// "Microsoft Basic Render Driver" reports this vendor id.
    const VENDOR_MICROSOFT: u32 = 0x1414;

    // Only the COM methods used here are named; the `_` arrays hold the ones in between so the
    // names land on the right vtable slots (IUnknown 0-2, IDXGIObject 3-6, then the interface's own).
    #[repr(C)]
    struct Unknown {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
    }
    #[repr(C)]
    struct FactoryVtbl {
        unknown: Unknown,
        _object: [usize; 4],  // IDXGIObject
        _factory: [usize; 5], // IDXGIFactory: EnumAdapters .. CreateSoftwareAdapter
        enum_adapters1: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    }
    #[repr(C)]
    struct AdapterVtbl {
        unknown: Unknown,
        _object: [usize; 4],  // IDXGIObject
        _adapter: [usize; 3], // IDXGIAdapter: EnumOutputs, GetDesc, CheckInterfaceSupport
        get_desc1: unsafe extern "system" fn(*mut c_void, *mut DXGI_ADAPTER_DESC1) -> i32,
    }

    pub fn enumerate() -> Vec<Adapter> {
        let mut found = Vec::new();
        unsafe {
            let mut factory: *mut c_void = std::ptr::null_mut();
            if CreateDXGIFactory1(&IID_IDXGIFACTORY1, &mut factory) != 0 || factory.is_null() {
                return found;
            }
            let factory_vtbl = &**(factory as *mut *const FactoryVtbl);

            let mut index = 0u32;
            loop {
                let mut adapter: *mut c_void = std::ptr::null_mut();
                // Non-zero ends the list (DXGI_ERROR_NOT_FOUND after the last adapter).
                if (factory_vtbl.enum_adapters1)(factory, index, &mut adapter) != 0 || adapter.is_null() {
                    break;
                }
                let adapter_vtbl = &**(adapter as *mut *const AdapterVtbl);
                let mut desc: DXGI_ADAPTER_DESC1 = std::mem::zeroed();
                if (adapter_vtbl.get_desc1)(adapter, &mut desc) == 0
                    && desc.Flags & FLAG_SOFTWARE == 0
                    && desc.VendorId != VENDOR_MICROSOFT
                {
                    let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
                    found.push(Adapter {
                        device_id: index as i32,
                        name: String::from_utf16_lossy(&desc.Description[..len]),
                        dedicated_vram: desc.DedicatedVideoMemory,
                    });
                }
                (adapter_vtbl.unknown.release)(adapter);
                index += 1;
            }
            (factory_vtbl.unknown.release)(factory);
        }
        found
    }
}
