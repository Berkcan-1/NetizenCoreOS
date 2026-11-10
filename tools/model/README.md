# Algoritma modelleri (Rust derleyicisi olmadan doğrulama)

Rust kodunun mantığının birebir Python portları ve Rust testlerinin aynı senaryolarla koşturulması.
Bunlar Rust testlerinin YERİNE GEÇMEZ; `cargo test` ile birlikte çalıştırılır.

    python3 tools/model/kpt_model.py        # nc-mem/src/kpt.rs
    python3 tools/model/elf_abi_model.py    # nc-elf (gerçek faz6_app.elf dahil)
    python3 tools/model/draw_model.py       # nc-abi/src/draw.rs

Faz 6 test ELF'i: `python3 kernel/faz6/gen.py` (GNU as + ld gerekir; çıktı depoya dahildir).
