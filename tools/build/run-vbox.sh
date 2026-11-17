run_vbox() {
    need VBoxManage
    local fw_mode="${VBOX_FIRMWARE:-efi}"

    if ! VBoxManage showvminfo "$VM_NAME" >/dev/null 2>&1; then
        VBoxManage createvm --name "$VM_NAME" --ostype Other_64 --register
        VBoxManage modifyvm "$VM_NAME" --memory 2048 --vram 256 --firmware "$fw_mode" \
            --graphicscontroller vboxvga --nic1 nat --nictype1 82540EM \
            --boot1 dvd --boot2 disk --boot3 none --boot4 none
        VBoxManage storagectl "$VM_NAME" --name IDE --add ide
    else
        VBoxManage modifyvm "$VM_NAME" --firmware "$fw_mode"
    fi

    VBoxManage modifyvm "$VM_NAME" --audio-enabled on --audio-driver default --audio-controller ac97 --audio-out on 2>/dev/null \
        || VBoxManage modifyvm "$VM_NAME" --audio default --audiocontroller ac97 --audioout on 2>/dev/null \
        || echo "UYARI: VirtualBox ses ayari yapilamadi; sanal makinede AC'97 sesini elle acin." >&2

    if [ ! -f "$BUILD/vbox-disk.vdi" ]; then
        echo "VirtualBox icin Hibrit (BIOS + UEFI) sabit disk hazirlaniyor..."
        local raw_disk="$BUILD/vbox-raw.img"
        rm -f "$raw_disk"
        prepare_hybrid_disk "$raw_disk"
        VBoxManage convertfromraw "$raw_disk" "$BUILD/vbox-disk.vdi" --format VDI
        rm -f "$raw_disk"
        echo "Hibrit vbox-disk.vdi olusturuldu."
    fi

    VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 0 --device 0 \
        --type hdd --medium "$BUILD/vbox-disk.vdi"

    # Eger VDI diske kurulum yapildiysa ("NETIZEN" imzasi varsa), ESP tablosunu onar ve ISO'yu cikar
    local check_raw="$BUILD/vbox-head.bin"
    rm -f "$check_raw"
    VBoxManage internalcommands converttoraw -start 0 -size 1024 "$BUILD/vbox-disk.vdi" "$check_raw" >/dev/null 2>&1 || true
    local vbox_sig=""
    if [ -f "$check_raw" ]; then
        vbox_sig=$(dd if="$check_raw" bs=1 count=7 skip=3 2>/dev/null | tr -d '\0' || true)
        rm -f "$check_raw"
    fi

    if [ "$vbox_sig" = "NETIZEN" ] && [ "${FORCE_ISO:-0}" != "1" ]; then
        echo "VirtualBox: Kurulu sistem tespit edildi, ISO cikariliyor ve diskten baslatiliyor..."
        VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 1 --device 0 \
            --type dvddrive --medium emptydrive >/dev/null 2>&1 || true
        VBoxManage modifyvm "$VM_NAME" --boot1 disk --boot2 dvd
    else
        VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 1 --device 0 \
            --type dvddrive --medium "$ISO"
        VBoxManage modifyvm "$VM_NAME" --boot1 dvd --boot2 disk
    fi

    VBoxManage startvm "$VM_NAME"
}
