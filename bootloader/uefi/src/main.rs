#![no_std]
#![no_main]

use core::ffi::c_void;
use core::panic::PanicInfo;
use nc_hal::{
    BootInfo, FramebufferInfo, MemoryRegion, BOOT_INFO_ADDRESS, BOOT_INFO_MAGIC, BOOT_INFO_VERSION,
};

type EfiStatus = usize;
type EfiHandle = *mut c_void;

const EFI_BUFFER_TOO_SMALL: EfiStatus = (1usize << (usize::BITS - 1)) | 5;
const EFI_LOADER_DATA: u32 = 2;
const EFI_ALLOCATE_MAX_ADDRESS: u32 = 1;
const EFI_MAX_PHYSICAL_ADDRESS: u64 = 0xFFFF_FFFF;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EfiGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x9042a9de,
    data2: 0x23dc,
    data3: 0x4a38,
    data4: [0x96, 0xfb, 0x7a, 0xde, 0xd0, 0x80, 0x51, 0x6a],
};

const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x964e5b22,
    data2: 0x6459,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};
const EFI_LOADED_IMAGE_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x5b1b31a1,
    data2: 0x9562,
    data3: 0x11d2,
    data4: [0x8e, 0x3f, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};
const EFI_FILE_INFO_GUID: EfiGuid = EfiGuid {
    data1: 0x09576e92,
    data2: 0x6d3f,
    data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

const EFI_ALLOCATE_ADDRESS: u32 = 2;
const EFI_FILE_MODE_READ: u64 = 1;
// Mirrors the current 260 MiB ESP's FAT32 data-cluster capacity, minus its three
// directory clusters. Keep this in sync with nc-storage::ESP_SECTORS/formatter.
const MAX_INSTALLER_EFI_BYTES: usize = 268_352_000;
// Reserves the flat kernel image plus linker-defined .bss and its 512 KiB stack.
// kernel/linker.ld contains an ASSERT that the complete runtime image fits here.
// OVMF/QEMU'da 0x800000'dan itibaren ACPI NVS bolgeleri var; 0x100000..0x800000 (7 MiB) bos ve
// kernel icin ayrilabilir. linker.ld ASSERT'i ayni degerle (0x700000) imaji bu sinira zorlar.
const KERNEL_RUNTIME_RESERVATION_BYTES: usize = 7 * 1024 * 1024;
const BOOT_REGION_UEFI_INSTALL_IMAGE: u32 = 0x4E43_0003;
const BOOT_REGION_STAGE2_IMAGE: u32 = 0x4E43_0004;

#[repr(C)]
struct EfiSimpleFileSystem {
    revision: u64,
    open_volume: unsafe extern "efiapi" fn(*mut EfiSimpleFileSystem, *mut *mut EfiFile) -> EfiStatus,
}

#[repr(C)]
struct EfiFile {
    revision: u64,
    open: unsafe extern "efiapi" fn(*mut EfiFile, *mut *mut EfiFile, *const u16, u64, u64) -> EfiStatus,
    close: unsafe extern "efiapi" fn(*mut EfiFile) -> EfiStatus,
    delete: usize,
    read: unsafe extern "efiapi" fn(*mut EfiFile, *mut usize, *mut c_void) -> EfiStatus,
    write: usize,
    get_position: usize,
    set_position: usize,
    get_info: unsafe extern "efiapi" fn(*mut EfiFile, *const EfiGuid, *mut usize, *mut c_void) -> EfiStatus,
    set_info: usize,
    flush: usize,
}

#[repr(C)]
struct EfiLoadedImageProtocol {
    revision: u32,
    parent_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
    device_handle: EfiHandle,
    file_path: *mut c_void,
    reserved: *mut c_void,
    load_options_size: u32,
    load_options: *mut c_void,
    image_base: *const u8,
    image_size: u64,
    image_code_type: u32,
    image_data_type: u32,
    unload: usize,
}

#[repr(C)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct EfiSystemTable {
    pub hdr: EfiTableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub console_in_handle: EfiHandle,
    pub con_in: *mut c_void,
    pub console_out_handle: EfiHandle,
    pub con_out: *mut c_void,
    pub standard_error_handle: EfiHandle,
    pub std_err: *mut c_void,
    pub runtime_services: *mut c_void,
    pub boot_services: *mut EfiBootServices,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EfiMemoryDescriptor {
    pub r#type: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
pub struct EfiBootServices {
    pub hdr: EfiTableHeader,
    pub raise_tpl: usize,
    pub restore_tpl: usize,
    pub allocate_pages: unsafe extern "efiapi" fn(allocate_type: u32, memory_type: u32, pages: usize, memory: *mut u64) -> EfiStatus,
    pub free_pages: unsafe extern "efiapi" fn(memory: u64, pages: usize) -> EfiStatus,
    pub get_memory_map: unsafe extern "efiapi" fn(
        memory_map_size: *mut usize,
        memory_map: *mut EfiMemoryDescriptor,
        map_key: *mut usize,
        descriptor_size: *mut usize,
        descriptor_version: *mut u32,
    ) -> EfiStatus,
    pub allocate_pool: usize,
    pub free_pool: usize,
    pub create_event: usize,
    pub set_timer: usize,
    pub wait_for_event: usize,
    pub signal_event: usize,
    pub close_event: usize,
    pub check_event: usize,
    pub install_protocol_interface: usize,
    pub reinstall_protocol_interface: usize,
    pub uninstall_protocol_interface: usize,
    pub handle_protocol: unsafe extern "efiapi" fn(
        handle: EfiHandle,
        protocol: *const EfiGuid,
        interface: *mut *mut c_void,
    ) -> EfiStatus,
    pub reserved: usize,
    pub register_protocol_notify: usize,
    pub locate_handle: usize,
    pub locate_device_path: usize,
    pub install_configuration_table: usize,
    pub load_image: usize,
    pub start_image: usize,
    pub exit: usize,
    pub unload_image: usize,
    pub exit_boot_services:
        unsafe extern "efiapi" fn(image_handle: EfiHandle, map_key: usize) -> EfiStatus,
    pub get_next_monotonic_count: usize,
    pub stall: usize,
    pub set_watchdog_timer: usize,
    pub connect_controller: usize,
    pub disconnect_controller: usize,
    pub open_protocol: usize,
    pub close_protocol: usize,
    pub open_protocol_information: usize,
    pub protocols_per_handle: usize,
    pub locate_handle_buffer: usize,
    pub locate_protocol: unsafe extern "efiapi" fn(
        protocol: *const EfiGuid,
        registration: *mut c_void,
        interface: *mut *mut c_void,
    ) -> EfiStatus,
}

#[repr(C)]
pub struct EfiGopModeInfo {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: u32,
    pub pixel_information: [u32; 4],
    pub pixels_per_scan_line: u32,
}

#[repr(C)]
pub struct EfiGopMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *mut EfiGopModeInfo,
    pub size_of_info: usize,
    pub frame_buffer_base: u64,
    pub frame_buffer_size: usize,
}

#[repr(C)]
pub struct EfiGraphicsOutputProtocol {
    pub query_mode: unsafe extern "efiapi" fn(
        this: *mut EfiGraphicsOutputProtocol,
        mode_number: u32,
        size_of_info: *mut usize,
        info: *mut *mut EfiGopModeInfo,
    ) -> EfiStatus,
    pub set_mode:
        unsafe extern "efiapi" fn(this: *mut EfiGraphicsOutputProtocol, mode_number: u32) -> EfiStatus,
    pub blt: usize,
    pub mode: *mut EfiGopMode,
}

static KERNEL_BLOB: &[u8] = include_bytes!(env!("NC_KERNEL_BIN"));
static STAGE2_BLOB: &[u8] = include_bytes!(env!("NC_STAGE2_BIN"));

#[inline]
fn mask_pos_size(mask: u32) -> (u8, u8) {
    if mask == 0 {
        return (0, 8);
    }
    let pos = mask.trailing_zeros() as u8;
    let size = mask.count_ones() as u8;
    (pos, size)
}

#[inline]
fn decode_pixel_format(fmt: u32, masks: &[u32; 4]) -> Option<((u8, u8), (u8, u8), (u8, u8))> {
    match fmt {
        0 => Some(((0, 8), (8, 8), (16, 8))), // PixelRedGreenBlueReserved8BitPerColor
        1 => Some(((16, 8), (8, 8), (0, 8))), // PixelBlueGreenRedReserved8BitPerColor
        2 => {
            let r = mask_pos_size(masks[0]);
            let g = mask_pos_size(masks[1]);
            let b = mask_pos_size(masks[2]);
            if r.1 > 0 && g.1 > 0 && b.1 > 0 {
                Some((r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

unsafe fn allocate_pages_below_4g(
    bs: *mut EfiBootServices,
    pages: usize,
) -> Option<(u64, usize)> {
    if pages == 0 { return None; }
    let mut address = EFI_MAX_PHYSICAL_ADDRESS;
    if ((*bs).allocate_pages)(EFI_ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, pages, &mut address) != 0
        || address == 0
        || address > EFI_MAX_PHYSICAL_ADDRESS
        || address.checked_add((pages as u64).checked_mul(4096)?.checked_sub(1)?)? > EFI_MAX_PHYSICAL_ADDRESS
    {
        return None;
    }
    Some((address, pages))
}

/// Reads the raw BOOTX64.EFI file from the same filesystem device as this image.
/// The file body is placed in firmware-allocated EfiLoaderData memory, prefixed by
/// the small legacy installer descriptor expected by the kernel setup screen.
unsafe fn load_installer_efi_image(
    image_handle: EfiHandle,
    bs: *mut EfiBootServices,
) -> Option<(u64, usize, usize)> {
    let mut loaded_ptr: *mut c_void = core::ptr::null_mut();
    if ((*bs).handle_protocol)(image_handle, &EFI_LOADED_IMAGE_PROTOCOL_GUID, &mut loaded_ptr) != 0
        || loaded_ptr.is_null()
    { return None; }
    let loaded = &*(loaded_ptr as *const EfiLoadedImageProtocol);

    let mut fs_ptr: *mut c_void = core::ptr::null_mut();
    if ((*bs).handle_protocol)(loaded.device_handle, &EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID, &mut fs_ptr) != 0
        || fs_ptr.is_null()
    { return None; }
    let fs = fs_ptr as *mut EfiSimpleFileSystem;
    let mut root: *mut EfiFile = core::ptr::null_mut();
    if ((*fs).open_volume)(fs, &mut root) != 0 || root.is_null() { return None; }

    let path = [
        b'\\' as u16, b'E' as u16, b'F' as u16, b'I' as u16, b'\\' as u16,
        b'B' as u16, b'O' as u16, b'O' as u16, b'T' as u16, b'\\' as u16,
        b'B' as u16, b'O' as u16, b'O' as u16, b'T' as u16, b'X' as u16,
        b'6' as u16, b'4' as u16, b'.' as u16, b'E' as u16, b'F' as u16,
        b'I' as u16, 0,
    ];
    let mut file: *mut EfiFile = core::ptr::null_mut();
    if ((*root).open)(root, &mut file, path.as_ptr(), EFI_FILE_MODE_READ, 0) != 0 || file.is_null() {
        ((*root).close)(root);
        return None;
    }

    // EFI_FILE_INFO: FileSize is the u64 at byte offset 8. The requested filename is
    // short, so a bounded stack buffer is sufficient; reject non-conforming responses.
    let mut info = [0u64; 32];
    let mut info_size = core::mem::size_of_val(&info);
    let info_status = ((*file).get_info)(file, &EFI_FILE_INFO_GUID, &mut info_size, info.as_mut_ptr() as *mut c_void);
    if info_status != 0 || info_size < 16 || info_size > core::mem::size_of_val(&info) {
        ((*file).close)(file);
        ((*root).close)(root);
        return None;
    }
    let info_bytes = core::slice::from_raw_parts(info.as_ptr() as *const u8, info_size);
    let file_size_u64 = match info_bytes[8..16].try_into() {
        Ok(raw) => u64::from_le_bytes(raw),
        Err(_) => {
            ((*file).close)(file);
            ((*root).close)(root);
            return None;
        }
    };
    if file_size_u64 == 0 || file_size_u64 > MAX_INSTALLER_EFI_BYTES as u64 {
        ((*file).close)(file);
        ((*root).close)(root);
        return None;
    }
    let file_size = file_size_u64 as usize;

    let payload_size = match 8usize.checked_add(file_size) {
        Some(size) => size,
        None => {
            ((*file).close)(file);
            ((*root).close)(root);
            return None;
        }
    };
    let pages = match payload_size.checked_add(4095).map(|v| v / 4096) {
        Some(count) if count != 0 => count,
        _ => {
            ((*file).close)(file);
            ((*root).close)(root);
            return None;
        }
    };
    let (address, allocated_pages) = match allocate_pages_below_4g(bs, pages) {
        Some(allocation) => allocation,
        None => {
            ((*file).close)(file);
            ((*root).close)(root);
            return None;
        }
    };
    let dst = address as *mut u8;
    core::ptr::write_unaligned(dst as *mut u32, 0x5545_4649); // "UEFI"
    core::ptr::write_unaligned(dst.add(4) as *mut u32, file_size as u32);

    let mut copied = 0usize;
    while copied < file_size {
        let mut amount = file_size - copied;
        let status = ((*file).read)(file, &mut amount, dst.add(8 + copied) as *mut c_void);
        if status != 0 || amount == 0 || amount > file_size - copied {
            let _ = ((*bs).free_pages)(address, allocated_pages);
            ((*file).close)(file);
            ((*root).close)(root);
            return None;
        }
        copied = match copied.checked_add(amount) {
            Some(next) => next,
            None => {
                let _ = ((*bs).free_pages)(address, allocated_pages);
                ((*file).close)(file);
                ((*root).close)(root);
                return None;
            }
        };
    }

    ((*file).close)(file);
    ((*root).close)(root);
    Some((address, allocated_pages, payload_size))
}

#[inline(always)]
unsafe fn dbg(c: u8) {
    core::arch::asm!("out 0xE9, al", in("al") c, options(nomem, nostack));
}

#[inline(always)]
unsafe fn halt(c: u8) -> ! {
    dbg(c);
    loop { core::arch::asm!("cli", "hlt", options(nomem, nostack)); }
}

unsafe fn dbg_hex(mut v: u64) {
    dbg(b'0'); dbg(b'x');
    let mut i = 0;
    while i < 16 {
        let n = ((v >> 60) & 0xF) as u8;
        dbg(if n < 10 { b'0' + n } else { b'a' + (n - 10) });
        v <<= 4;
        i += 1;
    }
}

/// Teşhis: 0x100000'dan başlayarak kaç sayfa ayrılabildiğini ve 0..32 MiB aralığındaki
/// bellek haritasını debugcon'a yazar. Çıktı: "Rsayfa" sonra satır satır "T<tip> <baslangic> <sayfa>".
unsafe fn dump_low_memory(bs: *mut EfiBootServices, wanted_pages: usize) {
    let mut ok = 0usize;
    let mut a = 0x0010_0000u64;
    while ok < wanted_pages {
        let mut want = a;
        if ((*bs).allocate_pages)(EFI_ALLOCATE_ADDRESS, EFI_LOADER_DATA, 1, &mut want) != 0 { break; }
        a += 4096;
        ok += 1;
    }
    dbg(b'\n'); dbg(b'R'); dbg_hex(ok as u64); dbg(b' '); dbg_hex(a); dbg(b'\n');

    let mut buf = [0u64; 4096]; // 32 KiB
    let mut size = core::mem::size_of_val(&buf);
    let mut key = 0usize;
    let mut dsz = 0usize;
    let mut ver = 0u32;
    let st = ((*bs).get_memory_map)(&mut size, buf.as_mut_ptr() as *mut EfiMemoryDescriptor, &mut key, &mut dsz, &mut ver);
    if st != 0 || dsz < core::mem::size_of::<EfiMemoryDescriptor>() { dbg(b'?'); return; }
    let base = buf.as_ptr() as *const u8;
    let mut off = 0usize;
    while off + dsz <= size {
        let d = core::ptr::read_unaligned(base.add(off) as *const EfiMemoryDescriptor);
        if d.physical_start < 0x0200_0000 {
            dbg(b'T'); dbg_hex(d.r#type as u64); dbg(b' ');
            dbg_hex(d.physical_start); dbg(b' ');
            dbg_hex(d.number_of_pages); dbg(b'\n');
        }
        off += dsz;
    }
}

#[no_mangle]
pub unsafe extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    dbg(b'1');
    let bs = (*system_table).boot_services;

    // Reserve all physical buffers before ExitBootServices. The linked kernel must
    // execute at 1 MiB, while the installer payloads may live anywhere below 4 GiB.
    let kernel_pages = KERNEL_RUNTIME_RESERVATION_BYTES / 4096;
    if KERNEL_BLOB.is_empty() || KERNEL_BLOB.len() > KERNEL_RUNTIME_RESERVATION_BYTES {
        halt(b'K');
    }
    let mut kernel_address = 0x0010_0000u64;
    if ((*bs).allocate_pages)(EFI_ALLOCATE_ADDRESS, EFI_LOADER_DATA, kernel_pages, &mut kernel_address) != 0
        || kernel_address != 0x0010_0000
    {
        dump_low_memory(bs, kernel_pages);
        halt(b'A');
    }

    if STAGE2_BLOB.len() != 4096 {
        halt(b'S');
    }
    let (stage2_address, _stage2_pages) = match allocate_pages_below_4g(bs, 1) {
        Some(allocation) => allocation,
        None => halt(b'B'),
    };
    core::ptr::copy_nonoverlapping(STAGE2_BLOB.as_ptr(), stage2_address as *mut u8, STAGE2_BLOB.len());

    dbg(b'2');
    let installer_image = load_installer_efi_image(image_handle, bs);
    dbg(b'3');

    // 2. GOP Grafik Protokolünü Al ve En Uygun Ekran Modunu Seç
    let mut gop_ptr: *mut c_void = core::ptr::null_mut();
    let status = ((*bs).locate_protocol)(
        &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        core::ptr::null_mut(),
        &mut gop_ptr,
    );
    if status != 0 || gop_ptr.is_null() {
        halt(b'G');
    }
    dbg(b'4');

    let gop = gop_ptr as *mut EfiGraphicsOutputProtocol;
    let max_mode = (*(*gop).mode).max_mode;

    const PREF_MODES: [(u32, u32); 6] = [
        (1280, 720),
        (1024, 768),
        (1920, 1080),
        (1366, 768),
        (1280, 800),
        (800, 600),
    ];
    let mut best_mode: Option<u32> = None;
    let mut best_rank: usize = usize::MAX;
    let mut fallback_any_rgb: Option<u32> = None;

    for m in 0..max_mode {
        let mut info_size = 0usize;
        let mut info_ptr: *mut EfiGopModeInfo = core::ptr::null_mut();
        if ((*gop).query_mode)(gop, m, &mut info_size, &mut info_ptr) == 0 && !info_ptr.is_null() {
            let w = (*info_ptr).horizontal_resolution;
            let h = (*info_ptr).vertical_resolution;
            let fmt = (*info_ptr).pixel_format;
            let masks = (*info_ptr).pixel_information;
            if decode_pixel_format(fmt, &masks).is_some() {
                if fallback_any_rgb.is_none() {
                    fallback_any_rgb = Some(m);
                }
                for (rank, &(pw, ph)) in PREF_MODES.iter().enumerate() {
                    if w == pw && h == ph && rank < best_rank {
                        best_rank = rank;
                        best_mode = Some(m);
                        break;
                    }
                }
                if best_rank == 0 {
                    break;
                }
            }
        }
    }

    if let Some(m) = best_mode.or(fallback_any_rgb) {
        let _ = ((*gop).set_mode)(gop, m);
    }
    dbg(b'5');

    let mode = (*gop).mode;
    if mode.is_null() || (*mode).info.is_null() || (*mode).frame_buffer_base == 0 {
        halt(b'M');
    }

    let info = (*mode).info;
    let ((r_pos, r_size), (g_pos, g_size), (b_pos, b_size)) =
        match decode_pixel_format((*info).pixel_format, &(*info).pixel_information) {
            Some(channels) => channels,
            None => halt(b'F'),
        };

    let pitch = match (*info).pixels_per_scan_line.checked_mul(4) {
        Some(p) if p >= (*info).horizontal_resolution.saturating_mul(4) => p,
        _ => halt(b'P'),
    };

    let fb_base = (*mode).frame_buffer_base;
    let fb_info = FramebufferInfo {
        address: fb_base,
        pitch,
        width: (*info).horizontal_resolution,
        height: (*info).vertical_resolution,
        bpp: 32,
        red_position: r_pos,
        red_size: r_size,
        green_position: g_pos,
        green_size: g_size,
        blue_position: b_pos,
        blue_size: b_size,
        reserved: 1, // 1 = UEFI
    };

    // 3. GetMemoryMap buffer is dynamically allocated below 4 GiB so the kernel's
    // current identity-map can read it after ExitBootServices. Every allocation/free
    // changes the map, therefore the map is always fetched again before EBS.
    let mut mmap_buffer: *mut u8 = core::ptr::null_mut();
    let mut mmap_physical = 0u64;
    let mut mmap_pages = 0usize;
    let mut mmap_capacity = 0usize;
    let mut mmap_size = 0usize;
    let mut map_key = 0usize;
    let mut desc_size = 0usize;
    let mut desc_ver = 0u32;
    let mut exited = false;

    for _ in 0..16 {
        let mut requested = mmap_capacity;
        let mmap_status = ((*bs).get_memory_map)(
            &mut requested,
            mmap_buffer as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut desc_size,
            &mut desc_ver,
        );

        if mmap_status == EFI_BUFFER_TOO_SMALL {
            if desc_size < core::mem::size_of::<EfiMemoryDescriptor>() {
                desc_size = core::mem::size_of::<EfiMemoryDescriptor>();
            }
            let slack = desc_size.checked_mul(32).and_then(|v| v.checked_add(4096)).unwrap_or(usize::MAX);
            let needed = requested.checked_add(slack).unwrap_or(0);
            let new_pages = needed.checked_add(4095).map(|v| v / 4096).unwrap_or(0);
            let new_capacity = new_pages.checked_mul(4096).unwrap_or(0);
            if new_pages == 0 || new_capacity == 0 { break; }

            let mut new_address = EFI_MAX_PHYSICAL_ADDRESS;
            if ((*bs).allocate_pages)(EFI_ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, new_pages, &mut new_address) != 0
                || new_address == 0 || new_address > EFI_MAX_PHYSICAL_ADDRESS
            { break; }

            if mmap_pages != 0 {
                let _ = ((*bs).free_pages)(mmap_physical, mmap_pages);
            }
            mmap_physical = new_address;
            mmap_pages = new_pages;
            mmap_capacity = new_capacity;
            mmap_buffer = mmap_physical as *mut u8;
            continue;
        }

        if mmap_status != 0 || mmap_buffer.is_null()
            || desc_size < core::mem::size_of::<EfiMemoryDescriptor>()
            || requested == 0 || requested > mmap_capacity || requested % desc_size != 0
        { break; }

        mmap_size = requested;
        // No allocation or other Boot Service is called between this map key and EBS.
        if ((*bs).exit_boot_services)(image_handle, map_key) == 0 {
            exited = true;
            break;
        }
    }

    if !exited || desc_size == 0 || mmap_buffer.is_null() || mmap_size % desc_size != 0 {
        halt(b'X');
    }

    // 4. ExitBootServices SONRASI: Kesmeleri Kapat (Artık hiçbir Boot Service çağrılamaz!)
    core::arch::asm!("cli", options(nomem, nostack));
    dbg(b'6');

    // Convert descriptors in place into the smaller BootInfo MemoryRegion format.
    // The EFI descriptor stride is at least MemoryRegion's size, so forward compaction
    // cannot overwrite a descriptor that has not yet been read.
    let raw_regions_ptr = mmap_buffer as *mut MemoryRegion;
    let total_descs = mmap_size / desc_size;
    let mut raw_count = 0usize;

    for i in 0..total_descs {
        let desc_ptr = mmap_buffer.add(i * desc_size) as *const EfiMemoryDescriptor;
        let d = core::ptr::read_unaligned(desc_ptr);
        let len = d.number_of_pages.saturating_mul(4096);
        if len == 0 {
            continue;
        }
        // EfiLoaderCode(1), EfiLoaderData(2), EfiBootServicesCode(3), EfiBootServicesData(4), EfiConventionalMemory(7)
        // BootServicesCode/Data become reclaimable only after ExitBootServices succeeds.
        let r_type = if matches!(d.r#type, 3 | 4 | 7) { 1 } else { 2 };
        core::ptr::write(
            raw_regions_ptr.add(raw_count),
            MemoryRegion {
                base: d.physical_start,
                length: len,
                region_type: r_type,
                acpi_extended: 1,
            },
        );
        raw_count += 1;
    }

    for i in 1..raw_count {
        let item = core::ptr::read(raw_regions_ptr.add(i));
        let mut j = i;
        while j > 0 && (*raw_regions_ptr.add(j - 1)).base > item.base {
            core::ptr::write(
                raw_regions_ptr.add(j),
                core::ptr::read(raw_regions_ptr.add(j - 1)),
            );
            j -= 1;
        }
        core::ptr::write(raw_regions_ptr.add(j), item);
    }

    let mut merged_count = 0usize;
    for i in 0..raw_count {
        let cur = core::ptr::read(raw_regions_ptr.add(i));
        if merged_count > 0 {
            let prev = &mut *raw_regions_ptr.add(merged_count - 1);
            let prev_end = prev.base.saturating_add(prev.length);
            if prev.region_type == cur.region_type && cur.base <= prev_end {
                let new_end = prev_end.max(cur.base.saturating_add(cur.length));
                prev.length = new_end.saturating_sub(prev.base);
                continue;
            }
        }
        core::ptr::write(raw_regions_ptr.add(merged_count), cur);
        merged_count += 1;
    }

    let mut final_count = merged_count;
    // Custom reserved descriptors publish installer buffers through the existing
    // BootInfo memory-region array without changing the missing nc-hal struct layout.
    if let Some((image_address, _image_pages, image_bytes)) = installer_image {
        core::ptr::write(
            raw_regions_ptr.add(final_count),
            MemoryRegion { base: image_address, length: image_bytes as u64, region_type: BOOT_REGION_UEFI_INSTALL_IMAGE, acpi_extended: 0 },
        );
        final_count += 1;
    }
    core::ptr::write(
        raw_regions_ptr.add(final_count),
        MemoryRegion { base: stage2_address, length: STAGE2_BLOB.len() as u64, region_type: BOOT_REGION_STAGE2_IMAGE, acpi_extended: 0 },
    );
    final_count += 1;
    if final_count > u32::MAX as usize {
        halt(b'N');
    }

    // BootInfo retains the UEFI memory map buffer and special image descriptors.
    // All three payload regions are EfiLoaderData (reserved), never PMM-usable.
    let boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        framebuffer: fb_info,
        memory_map_addr: mmap_physical,
        memory_map_count: final_count as u32,
    };
    core::ptr::write(BOOT_INFO_ADDRESS as *mut BootInfo, boot_info);

    // The kernel destination was explicitly reserved with AllocateAddress before EBS.
    let kernel_dst = 0x0010_0000 as *mut u8;
    core::ptr::copy_nonoverlapping(KERNEL_BLOB.as_ptr(), kernel_dst, KERNEL_BLOB.len());

    dbg(b'J');
    // 6. Çekirdeğe Sorunsuz Atlayış: Yığını kur ve doğrudan 0x100000 (_start) noktasına zıpla
    core::arch::asm!(
        "mov rsp, 0x90000",
        "mov rax, 0x100000",
        "jmp rax",
        options(noreturn)
    );
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}