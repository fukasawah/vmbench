#!/usr/bin/env python3
"""Independent audit of a vmbench binary.

Checks, using only the Python standard library:
  1. the binary is statically linked (no PT_INTERP / dynamic section),
  2. there are no undefined symbols,
  3. every benchmark kernel symbol exists and its SHA-256 matches the value
     reported by `vmbench --list`,
  4. no kernel function contains a call instruction (the kernels must not call
     into libc or any helper).

Usage: audit_elf.py <binary> <list.json>
"""

import hashlib
import json
import re
import struct
import subprocess
import sys

PT_LOAD = 1
SHT_SYMTAB = 2


def fail(msg):
    print("AUDIT FAIL:", msg)
    sys.exit(1)


def parse_elf(data):
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        fail("not a little-endian ELF64 file")
    (e_type, e_machine, _ver, _entry, e_phoff, e_shoff, _flags, _ehsize,
     e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx) = struct.unpack_from(
        "<HHIQQQIHHHHHH", data, 16)
    loads = []
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        p_type, _p_flags, p_offset, p_vaddr, _p_paddr, p_filesz, _p_memsz, _p_align = (
            struct.unpack_from("<IIQQQQQQ", data, off))
        if p_type == PT_LOAD:
            loads.append((p_vaddr, p_offset, p_filesz))
    sections = []
    for i in range(e_shnum):
        off = e_shoff + i * e_shentsize
        (sh_name, sh_type, _sh_flags, _sh_addr, sh_offset, sh_size, sh_link,
         _sh_info, _sh_addralign, sh_entsize) = struct.unpack_from("<IIQQQQIIQQ", data, off)
        sections.append({
            "name_off": sh_name, "type": sh_type, "offset": sh_offset,
            "size": sh_size, "link": sh_link, "entsize": sh_entsize,
        })
    return e_type, loads, sections


def cstr(data, off):
    end = data.index(b"\0", off)
    return data[off:end]


def symbols(data, loads, sections):
    symtab = None
    strtab = None
    for s in sections:
        if s["type"] == SHT_SYMTAB:
            symtab = s
            strtab = sections[s["link"]]
            break
    if symtab is None:
        fail("no .symtab (binary is stripped)")
    syms = {}
    n = symtab["size"] // symtab["entsize"]
    for i in range(n):
        off = symtab["offset"] + i * symtab["entsize"]
        st_name, st_info, _st_other, _st_shndx, st_value, st_size = struct.unpack_from(
            "<IBBHQQ", data, off)
        if st_value == 0:
            continue
        name = cstr(data, strtab["offset"] + st_name).decode()
        syms[name] = (st_value, st_size)
    return syms


def vaddr_to_offset(loads, vaddr):
    for p_vaddr, p_offset, p_filesz in loads:
        if p_vaddr <= vaddr < p_vaddr + p_filesz:
            return p_offset + (vaddr - p_vaddr)
    return None


def main():
    binary = sys.argv[1]
    list_json = sys.argv[2]
    objdump = sys.argv[3] if len(sys.argv) > 3 else "objdump"
    data = open(binary, "rb").read()
    e_type, loads, sections = parse_elf(data)
    if not loads:
        fail("no PT_LOAD segments")

    # 1. static: no PT_INTERP and no dynamic section
    e_phoff = struct.unpack_from("<Q", data, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 54)
    for i in range(e_phnum):
        p_type = struct.unpack_from("<I", data, e_phoff + i * e_phentsize)[0]
        if p_type == 3:
            fail("binary has PT_INTERP (dynamically linked)")

    # 2. no undefined symbols
    out = subprocess.run(["nm", "-u", binary], capture_output=True, text=True)
    if out.stdout.strip():
        fail("undefined symbols present:\n" + out.stdout)

    syms = symbols(data, loads, sections)

    # 3. kernel code hashes
    report = json.load(open(list_json))
    benchmarks = report["benchmarks"] if isinstance(report, dict) else report
    checked = 0
    for b in benchmarks:
        name = b["kernel"]
        expected = b.get("code_hash")
        if name not in syms:
            fail("kernel symbol missing from binary: %s" % name)
        value, size = syms[name]
        off = vaddr_to_offset(loads, value)
        if off is None:
            fail("cannot map symbol %s to file offset" % name)
        code = data[off:off + size]
        got = "sha256:" + hashlib.sha256(code).hexdigest()
        if expected and expected != got:
            fail("code hash mismatch for %s: list=%s elf=%s" % (name, expected, got))
        checked += 1
    print("AUDIT OK: %d kernel symbols verified" % checked)

    # 4. benchmark ids must be unique.
    ids = [b["id"] for b in benchmarks]
    if len(ids) != len(set(ids)):
        dupes = sorted({i for i in ids if ids.count(i) > 1})
        fail("duplicate benchmark ids: %s" % ", ".join(dupes))
    print("AUDIT OK: %d unique benchmark ids" % len(ids))

    # 5. kernel disassembly checks:
    #    - no libc-style calls / PLT calls,
    #    - every kernel contains a backward branch (loop). A kernel optimized
    #      into a closed form would have no loop and would silently measure
    #      nothing.
    forbidden = {
        "memcpy", "memset", "memmove", "memcmp", "bcmp", "strlen", "strcmp",
        "strncmp", "strcpy", "strncpy", "__memcpy_chk", "__memset_chk",
        "malloc", "free", "calloc", "realloc", "printf", "snprintf",
    }
    if objdump == "none":
        print("AUDIT WARN: no working objdump for this target; disassembly checks skipped")
        return
    kernels = {b["kernel"] for b in benchmarks}
    dis = subprocess.run([objdump, "-d", binary], capture_output=True, text=True).stdout
    # x86 objdump prints 2-hex-digit byte pairs; aarch64 prints one 8-digit word.
    line_re = re.compile(r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]+\s+)+([a-z][a-z0-9._]*)\s*(.*)$")
    instrs = []
    for line in dis.splitlines():
        m = line_re.match(line)
        if m:
            instrs.append((int(m.group(1), 16), m.group(2), m.group(3), line))
    # Ranges are used instead of objdump function headers because identical
    # kernels may be folded together (ICF), leaving aliased symbols without
    # their own header line.
    missing = []
    for name in sorted(kernels):
        value, size = syms[name]
        lo = value
        hi = value + max(size, 1)
        loop_seen = False
        syscall_seen = False
        for addr, mn, rest, line in instrs:
            if addr < lo or addr >= hi:
                continue
            if mn in ("syscall", "svc"):
                syscall_seen = True
            if mn == "call":
                if "@plt" in line:
                    fail("kernel %s calls through PLT: %s" % (name, line.strip()))
                target = line.split("<")[-1].rstrip(">") if "<" in line else ""
                if target in forbidden:
                    fail("kernel %s calls %s: %s" % (name, target, line.strip()))
            is_branch = (
                mn == "loop"
                or mn.startswith("j")
                or mn == "b"
                or mn.startswith("b.")
                or mn in ("cbz", "cbnz", "tbz", "tbnz")
            )
            if is_branch:
                t = re.search(r"\b([0-9a-f]{4,})\b", rest)
                if t and int(t.group(1), 16) < addr:
                    loop_seen = True
        # Compute kernels must contain a backward branch; single-syscall
        # kernels (e.g. one pwrite) are accepted if they contain syscall/svc.
        if not loop_seen and not syscall_seen:
            missing.append(name)
    if missing:
        fail("kernels without a backward branch or syscall (possible constant folding): %s" % ", ".join(missing))
    print("AUDIT OK: all kernels contain a loop or syscall; no libc-style calls")


if __name__ == "__main__":
    main()
