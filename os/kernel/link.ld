ENTRY(start)

MEMORY {
    high : ORIGIN = 25M, LENGTH = 0x7000000
}

SECTIONS {
    /*
     * load the kernel to the following address
     *
     * a QEMU VM with OVMF roughly has the following memory layout:
     *   0KB to 640KB: free
     * 640KB to 1MB:   devices
     *   1MB to 8MB:   free
     *   8MB to 9MB:   ACPI
     *   9MB to 24MB:  Boot Services
     */
    . = 25M;

    ___KERNEL_DATA_START__ = .;
    .boot : ALIGN (4K) {
        ___BOOT_START__ = .;
        *(.multiboot2_header*)
        ___BOOT_END__ = .;
    } AT> high
    
    .text : ALIGN (4K) { 
        ___TEXT_START__ = .;
        *(.text*)
        ___TEXT_END__ = .;
    } AT> high
    
    ___BSS_START__ = .;
    .bss : ALIGN (4K) {
        ___BSS_START_INNER__ = .;
        *(".bss*")
        ___BSS_END_INNER__ = .;
    } AT> high
    ___BSS_END__ = .;
    
    .data : ALIGN (4K) {
        ___DATA_START__ = .;
        *(.data*)
        ___DATA_END__ = .;
    } AT> high
    ___KERNEL_DATA_END__ = .;
    
    /* The visible_from_usermode section needs to be mapped from its lower
    region in the kernel region into a higher region starting at 512GiB to
    make it visible in the userspace in a different entry of the top level
    page table */
    
    .visible_from_usermode 0x8000000000 : ALIGN (4K) {
        ___VISIBLE_FROM_USERMODE_START_INNER__ = .;
        *(.visible_from_usermode*)
        ___VISIBLE_FROM_USERMODE_END_INNER__ = .;
    } AT> high
    
    /* Setting . = ALIGN (4K); before the .visible_from_usermode section
    will be ignored, therefore we need to set ALIGN (4K) as part of the
    section, and compute ___VISIBLE_FROM_USERMODE_START__ manually */
    ___VISIBLE_FROM_USERMODE_START__  = LOADADDR(.visible_from_usermode);
    
    /* TODO: Check why ___VISIBLE_FROM_USERMODE_END__ = .; doesn't work here */
    ___VISIBLE_FROM_USERMODE_END__ = ___VISIBLE_FROM_USERMODE_START__ + SIZEOF (.visible_from_usermode);
    
}
