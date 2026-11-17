build_drivers() {
    mkdir -p "$BUILD/drivers"
    cat > "$BUILD/drivers/generic-display.efra.src" << 'EOF'
id=efra.display.generic
name=Netizen VBE/GOP Generic Turbo Framebuffer
version=2.1.0
file=generic-display.efra
class=display
vendor_id=0xFFFF
device_id=0xFFFF
pci_class=0x03
pci_subclass=0x00
write_combining=true
dma_burst=true
EOF
    printf "EFRA\x01\x00\x00\x00" > "$BUILD/drivers/generic-display.efra"
    echo "Harici .efra derleme alani hazir: $BUILD/drivers/generic-display.efra"
}
