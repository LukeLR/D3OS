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
    .boot : { *(.multiboot2_header) } AT> high
    
    . = ALIGN (4K);
    .text : { *(.text*) } AT> high
    
    . = ALIGN (4K);
    ___BSS_START__ = .;
   .bss : { *(".bss*") } AT> high
    ___BSS_END__ = .;
    
    . = ALIGN (4K);
    .data : { *(.data*) } AT> high
    
    . = ALIGN (4K);
    /* The visible_from_usermode section needs to be mapped from its lower
    region in the kernel region into a higher region starting at 512GiB to
    make it visible in the userspace in a different entry of the top level
    page table */
    ___VISIBLE_FROM_USERMODE_START__  = .;
    
    .visible_from_usermode 0x18000000000 : { *(.visible_from_usermode*) } AT> high
    
    /* TODO: Check why ___VISIBLE_FROM_USERMODE_END__ = .; doesn't work here */
    ___VISIBLE_FROM_USERMODE_END__ = ___VISIBLE_FROM_USERMODE_START__ + SIZEOF (.visible_from_usermode);
    ___KERNEL_DATA_END__ = ___VISIBLE_FROM_USERMODE_END__;
}
