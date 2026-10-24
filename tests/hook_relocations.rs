// The installed hook is a PE32+ DLL that Windows (ASLR) and Wine usually load away from its
// preferred base. Such an image may only use 64-bit (DIR64) base relocations: a 32-bit HIGHLOW
// entry is applied with a truncated delta. GNU ld emits them for native TLS offsets, which made
// every thread-local access in the Linux-built hook read invalid memory and crash the game.
#![cfg(feature = "hook")]

const ABSOLUTE: u8 = 0;
const DIR64: u8 = 10;

fn u16_at(image: &[u8], offset: usize) -> usize {
    u16::from_le_bytes(image[offset..offset + 2].try_into().unwrap()).into()
}

fn u32_at(image: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap()) as usize
}

/// Returns `(rva, type)` for every base relocation of a PE32+ file.
fn base_relocations(image: &[u8]) -> Vec<(usize, u8)> {
    let pe = u32_at(image, 0x3c);
    assert_eq!(&image[pe..pe + 4], b"PE\0\0", "not a PE file");
    let optional = pe + 24;
    assert_eq!(u16_at(image, optional), 0x20b, "not a PE32+ image");
    let sections = optional + u16_at(image, pe + 20);
    let file_offset = |rva: usize| {
        (0..u16_at(image, pe + 6))
            .map(|i| sections + i * 40)
            .find_map(|s| {
                let (virtual_address, size) = (u32_at(image, s + 12), u32_at(image, s + 16));
                (virtual_address..virtual_address + size.max(u32_at(image, s + 8)))
                    .contains(&rva)
                    .then(|| rva - virtual_address + u32_at(image, s + 20))
            })
            .unwrap_or_else(|| panic!("RVA {rva:#x} is outside every section"))
    };

    let directory = optional + 112 + 5 * 8;
    let (rva, size) = (u32_at(image, directory), u32_at(image, directory + 4));
    let mut relocations = Vec::new();
    if size == 0 {
        return relocations;
    }
    let (mut block, end) = (file_offset(rva), file_offset(rva) + size);
    while block < end {
        let (page, block_size) = (u32_at(image, block), u32_at(image, block + 4));
        assert!(block_size >= 8, "malformed relocation block");
        for entry in (block + 8..block + block_size).step_by(2) {
            let entry = u16_at(image, entry);
            relocations.push((page + (entry & 0xfff), (entry >> 12) as u8));
        }
        block += block_size;
    }
    relocations
}

fn unexpected(relocations: &[(usize, u8)]) -> Vec<String> {
    relocations
        .iter()
        .filter(|(_, kind)| !matches!(*kind, ABSOLUTE | DIR64))
        .map(|(rva, kind)| format!("type {kind} at RVA {rva:#x}"))
        .collect()
}

#[test]
fn hook_dll_uses_only_64_bit_base_relocations() {
    let image = std::fs::read(env!("CARGO_CDYLIB_FILE_HOOK_hook")).unwrap();
    let relocations = base_relocations(&image);
    assert!(
        relocations.iter().any(|(_, kind)| *kind == DIR64),
        "the hook must remain relocatable"
    );
    let unexpected = unexpected(&relocations);
    assert!(
        unexpected.is_empty(),
        "{} relocations would corrupt the hook when it is relocated, first: {:?}",
        unexpected.len(),
        &unexpected[..unexpected.len().min(5)]
    );
}

#[test]
fn relocation_check_reports_32_bit_entries() {
    // Minimal PE32+ image: one section holding a relocation block with a DIR64, a HIGHLOW
    // (type 3) and a padding entry for page 0x1000.
    let mut image = vec![0u8; 0x400];
    image[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    image[0x40..0x44].copy_from_slice(b"PE\0\0");
    image[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    image[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    let optional = 0x58;
    image[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    let directory = optional + 112 + 5 * 8;
    image[directory..directory + 4].copy_from_slice(&0x2000u32.to_le_bytes());
    image[directory + 4..directory + 8].copy_from_slice(&14u32.to_le_bytes());
    let section = optional + 240;
    for (offset, value) in [(8, 0x200u32), (12, 0x2000), (16, 0x200), (20, 0x200)] {
        image[section + offset..section + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let block = [0x1000u32.to_le_bytes(), 14u32.to_le_bytes()].concat();
    image[0x200..0x208].copy_from_slice(&block);
    for (i, entry) in [0xa010u16, 0x3020, 0x0000].iter().enumerate() {
        image[0x208 + i * 2..0x20a + i * 2].copy_from_slice(&entry.to_le_bytes());
    }

    let relocations = base_relocations(&image);
    assert_eq!(relocations, [(0x1010, 10), (0x1020, 3), (0x1000, 0)]);
    assert_eq!(unexpected(&relocations), ["type 3 at RVA 0x1020"]);
}
