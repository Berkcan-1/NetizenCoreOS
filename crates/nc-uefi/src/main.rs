#![no_std]
#![no_main]

use core::ffi::c_void;
use core::panic::PanicInfo;
use nc_hal::{
    BootInfo, FramebufferInfo, MemoryRegion, BOOT_INFO_ADDRESS, BOOT_INFO_MAGIC, BOOT_INFO_VERSION,
};

type EfiStatus = usize;
type EfiHandle = *mut c_void;

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

const EFI_SIMPLE_FS_GUID: EfiGuid = EfiGuid {
    data1: 0x964e5b22, data2: 0x6459, data3: 0x11d2,
    data4: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

#[repr(C)]
struct EfiSimpleFs {
    revision: u64,
    open_volume: unsafe extern "efiapi" fn(*mut EfiSimpleFs, *mut *mut EfiFile) -> EfiStatus,
}

#[repr(C)]
struct EfiFile {
    revision: u64,
    open: unsafe extern "efiapi" fn(*mut EfiFile, *mut *mut EfiFile, *const u16, u64, u64) -> EfiStatus,
    close: unsafe extern "efiapi" fn(*mut EfiFile) -> EfiStatus,
    delete: usize,
    read: unsafe extern "efiapi" fn(*mut EfiFile, *mut usize, *mut c_void) -> EfiStatus,
}

unsafe fn read_own_file(
    li: &EfiLoadedImageProtocol,
    bs: *mut EfiBootServices,
    dst: *mut u8,
    cap: usize,
) -> Option<usize> {
    let mut sfs_ptr: *mut c_void = core::ptr::null_mut();
    if ((*bs).handle_protocol)(li.device_handle, &EFI_SIMPLE_FS_GUID, &mut sfs_ptr) != 0
        || sfs_ptr.is_null()
    {
        return None;
    }
    let sfs = sfs_ptr as *mut EfiSimpleFs;
    let mut root: *mut EfiFile = core::ptr::null_mut();
    if ((*sfs).open_volume)(sfs, &mut root) != 0 || root.is_null() {
        return None;
    }
    let src = b"\\EFI\\BOOT\\BOOTX64.EFI";
    let mut path = [0u16; 32];
    for (i, &b) in src.iter().enumerate() {
        path[i] = b as u16;
    }
    let mut file: *mut EfiFile = core::ptr::null_mut();
    if ((*root).open)(root, &mut file, path.as_ptr(), 1, 0) != 0 || file.is_null() {
        ((*root).close)(root);
        return None;
    }
    let mut size = cap;
    let st = ((*file).read)(file, &mut size, dst as *mut c_void);
    ((*file).close)(file);
    ((*root).close)(root);
    if st == 0 && size > 0 { Some(size) } else { None }
}

const EFI_LOADED_IMAGE_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x5b1b31a1,
    data2: 0x9562,
    data3: 0x11d2,
    data4: [0x8e, 0x3f, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b],
};

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
    pub allocate_pages: usize,
    pub free_pages: usize,
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

#[repr(C, align(16))]
struct AlignedMmapBuf([u8; 65536]);

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

#[no_mangle]
pub unsafe extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    let bs = (*system_table).boot_services;

    // 1. BOOTX64.EFI ikilisini Boot Services HENÜZ AKTİFKEN oku ve 0x0040_0000 adresine kopyala
    let mut loaded_img_ptr: *mut c_void = core::ptr::null_mut();
    if ((*bs).handle_protocol)(
        image_handle,
        &EFI_LOADED_IMAGE_PROTOCOL_GUID,
        &mut loaded_img_ptr,
    ) == 0 && !loaded_img_ptr.is_null() {
        let li = &*(loaded_img_ptr as *const EfiLoadedImageProtocol);
        let base = li.image_base;
        let size = li.image_size as usize;
        if !base.is_null() && size > 0 && size <= 3 * 1024 * 1024 {
            let export_dst = 0x0040_0000 as *mut u8;
            if let Some(sz) = read_own_file(li, bs, export_dst.add(8), 3 * 1024 * 1024) {
            core::ptr::write_unaligned(export_dst as *mut u32, 0x5545_4649); // "UEFI"
            core::ptr::write_unaligned(export_dst.add(4) as *mut u32, sz as u32);
            }
    }
}

    // 2. GOP Grafik Protokolünü Al ve En Uygun Ekran Modunu Seç
    let mut gop_ptr: *mut c_void = core::ptr::null_mut();
    let status = ((*bs).locate_protocol)(
        &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        core::ptr::null_mut(),
        &mut gop_ptr,
    );
    if status != 0 || gop_ptr.is_null() {
        loop {
            core::arch::asm!("hlt");
        }
    }

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

    let mode = (*gop).mode;
    if mode.is_null() || (*mode).info.is_null() || (*mode).frame_buffer_base == 0 {
        loop {
            core::arch::asm!("hlt");
        }
    }

    let info = (*mode).info;
    let ((r_pos, r_size), (g_pos, g_size), (b_pos, b_size)) =
        match decode_pixel_format((*info).pixel_format, &(*info).pixel_information) {
            Some(channels) => channels,
            None => loop {
                core::arch::asm!("hlt");
            },
        };

    let pitch = match (*info).pixels_per_scan_line.checked_mul(4) {
        Some(p) if p >= (*info).horizontal_resolution.saturating_mul(4) => p,
        _ => loop {
            core::arch::asm!("hlt");
        },
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

    // 3. Bellek Haritasını Al ve ExitBootServices Döngüsü
    static mut ALIGNED_BUF: AlignedMmapBuf = AlignedMmapBuf([0u8; 65536]);
    let mut mmap_size = 0usize;
    let mut map_key = 0usize;
    let mut desc_size = 0usize;
    let mut desc_ver = 0u32;
    let mut exited = false;

    let aligned_buf_ptr = core::ptr::addr_of_mut!(ALIGNED_BUF) as *mut u8;

    for _ in 0..8 {
        mmap_size = core::mem::size_of::<AlignedMmapBuf>();
        let mmap_status = ((*bs).get_memory_map)(
            &mut mmap_size,
            aligned_buf_ptr as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut desc_size,
            &mut desc_ver,
        );
        if mmap_status != 0 || desc_size < core::mem::size_of::<EfiMemoryDescriptor>() {
            break;
        }
        if ((*bs).exit_boot_services)(image_handle, map_key) == 0 {
            exited = true;
            break;
        }
    }

    if !exited || desc_size == 0 {
        loop {
            core::arch::asm!("hlt");
        }
    }

    // 4. ExitBootServices SONRASI: Kesmeleri Kapat (Artık hiçbir Boot Service çağrılamaz!)
    core::arch::asm!("cli", options(nomem, nostack));

    // Bellek haritasını E820 formatında 0x0A00 adresine yerleştir
    const MAX_RAW_REGIONS: usize = 512;
    static mut RAW_REGIONS: [MemoryRegion; MAX_RAW_REGIONS] = [MemoryRegion {
        base: 0,
        length: 0,
        region_type: 0,
        acpi_extended: 0,
    }; MAX_RAW_REGIONS];

    let raw_regions_ptr = core::ptr::addr_of_mut!(RAW_REGIONS) as *mut MemoryRegion;
    let total_descs = (mmap_size / desc_size).min(MAX_RAW_REGIONS);
    let mut raw_count = 0usize;

    for i in 0..total_descs {
        let desc_ptr = aligned_buf_ptr.add(i * desc_size) as *const EfiMemoryDescriptor;
        let d = core::ptr::read_unaligned(desc_ptr);
        let len = d.number_of_pages.saturating_mul(4096);
        if len == 0 {
            continue;
        }
        // EfiLoaderCode(1), EfiLoaderData(2), EfiBootServicesCode(3), EfiBootServicesData(4), EfiConventionalMemory(7)
        let r_type = if matches!(d.r#type, 4 | 7) {
        1
    } else {
        2
    };
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

    let e820_target = 0x0A00 as *mut MemoryRegion;
    let final_count = merged_count.min(64);
    for i in 0..final_count {
        core::ptr::write(e820_target.add(i), core::ptr::read(raw_regions_ptr.add(i)));
    }

    // 5. BootInfo'yu 0x0900 (BOOT_INFO_ADDRESS) Adresine Yaz
    let boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        version: BOOT_INFO_VERSION,
        framebuffer: fb_info,
        memory_map_addr: 0x0A00,
        memory_map_count: final_count as u32,
    };
    core::ptr::write(BOOT_INFO_ADDRESS as *mut BootInfo, boot_info);

    // Stage 2 ve Kernel'i Belleğe Kopyala
    if STAGE2_BLOB.len() == 4096 {
        core::ptr::copy_nonoverlapping(STAGE2_BLOB.as_ptr(), 0x8800 as *mut u8, 4096);
    }

    let kernel_dst = 0x0010_0000 as *mut u8;
    core::ptr::copy_nonoverlapping(KERNEL_BLOB.as_ptr(), kernel_dst, KERNEL_BLOB.len());

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