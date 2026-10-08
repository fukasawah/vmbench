use crate::sha256::Sha256;
use crate::sys;

const PT_LOAD: u32 = 1;
const SHT_SYMTAB: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

pub struct SelfImage {
    data: &'static [u8],
}

impl SelfImage {
    pub fn load() -> Option<SelfImage> {
        let fd = sys::open(b"/proc/self/exe", sys::O_RDONLY | sys::O_CLOEXEC, 0).ok()?;
        let size = match sys::lseek(fd, 0, 2) {
            Ok(s) => s as usize,
            Err(_) => {
                let _ = sys::close(fd);
                return None;
            }
        };
        if size == 0 || size > 512 * 1024 * 1024 {
            let _ = sys::close(fd);
            return None;
        }
        let p = match sys::mmap(
            core::ptr::null_mut(),
            size,
            sys::PROT_READ,
            sys::MAP_PRIVATE,
            fd,
            0,
        ) {
            Ok(p) => p,
            Err(_) => {
                let _ = sys::close(fd);
                return None;
            }
        };
        let _ = sys::close(fd);
        let data = unsafe { core::slice::from_raw_parts(p, size) };
        Some(SelfImage { data })
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.data
    }

    fn ehdr(&self) -> Option<&Elf64Ehdr> {
        if self.data.len() < core::mem::size_of::<Elf64Ehdr>() {
            return None;
        }
        if &self.data[0..4] != b"\x7fELF" || self.data[4] != 2 || self.data[5] != 1 {
            return None;
        }
        Some(unsafe { &*(self.data.as_ptr() as *const Elf64Ehdr) })
    }

    pub fn whole_hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.data);
        h.finish()
    }

    fn vaddr_to_offset(&self, vaddr: u64) -> Option<(usize, usize)> {
        let e = self.ehdr()?;
        if e.e_phoff == 0 || e.e_phnum == 0 {
            return None;
        }
        for i in 0..e.e_phnum as usize {
            let off = e.e_phoff as usize + i * e.e_phentsize as usize;
            if off + core::mem::size_of::<Elf64Phdr>() > self.data.len() {
                return None;
            }
            let ph = unsafe { &*(self.data.as_ptr().add(off) as *const Elf64Phdr) };
            if ph.p_type != PT_LOAD || ph.p_filesz == 0 {
                continue;
            }
            if vaddr >= ph.p_vaddr && vaddr < ph.p_vaddr + ph.p_filesz {
                let fo = ph.p_offset + (vaddr - ph.p_vaddr);
                let max = (ph.p_filesz - (vaddr - ph.p_vaddr)) as usize;
                return Some((fo as usize, max));
            }
        }
        None
    }

    /// Returns (symtab offset, symtab size, strtab offset, strtab size, entsize).
    fn symtab(&self) -> Option<(usize, usize, usize, usize, usize)> {
        let e = self.ehdr()?;
        if e.e_shoff == 0 || e.e_shnum == 0 {
            return None;
        }
        for i in 0..e.e_shnum as usize {
            let off = e.e_shoff as usize + i * e.e_shentsize as usize;
            if off + core::mem::size_of::<Elf64Shdr>() > self.data.len() {
                return None;
            }
            let sh = unsafe { &*(self.data.as_ptr().add(off) as *const Elf64Shdr) };
            if sh.sh_type == SHT_SYMTAB && sh.sh_entsize as usize == core::mem::size_of::<Elf64Sym>()
            {
                let stroff = e.e_shoff as usize
                    + sh.sh_link as usize * e.e_shentsize as usize;
                if stroff + core::mem::size_of::<Elf64Shdr>() > self.data.len() {
                    return None;
                }
                let strsh = unsafe { &*(self.data.as_ptr().add(stroff) as *const Elf64Shdr) };
                return Some((
                    sh.sh_offset as usize,
                    sh.sh_size as usize,
                    strsh.sh_offset as usize,
                    strsh.sh_size as usize,
                    sh.sh_entsize as usize,
                ));
            }
        }
        None
    }

    fn cstr(&self, off: usize) -> &[u8] {
        let mut end = off;
        while end < self.data.len() && self.data[end] != 0 {
            end += 1;
        }
        &self.data[off..end]
    }

    /// Looks up a function symbol and returns (sha256 of machine code, size).
    pub fn symbol_hash(&self, name: &[u8]) -> Option<([u8; 32], u64)> {
        let (sym_off, sym_size, str_off, str_size, ent) = self.symtab()?;
        let count = sym_size / ent;
        let mut addr = None;
        let mut size = 0u64;
        for i in 0..count {
            let off = sym_off + i * ent;
            if off + core::mem::size_of::<Elf64Sym>() > self.data.len() {
                return None;
            }
            let sym = unsafe { &*(self.data.as_ptr().add(off) as *const Elf64Sym) };
            if sym.st_name as usize >= str_size {
                continue;
            }
            let n = self.cstr(str_off + sym.st_name as usize);
            if n == name {
                if sym.st_size == 0 || sym.st_value == 0 {
                    return None;
                }
                addr = Some(sym.st_value);
                size = sym.st_size;
                break;
            }
        }
        let addr = addr?;
        let (fo, max) = self.vaddr_to_offset(addr)?;
        let n = (size as usize).min(max);
        if n == 0 || fo + n > self.data.len() {
            return None;
        }
        let code = &self.data[fo..fo + n];
        Some((crate::sha256::hash(code), size))
    }

    pub fn has_symbol(&self, name: &[u8]) -> bool {
        self.symbol_hash(name).is_some()
    }
}
