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

    .boot :
    {
        *(.multiboot2_header)
    } AT> high

    .text :
    {
        *(.text*)
    } AT> high

   .bss :
    {
      ___BSS_START__ = .;
      *(".bss*")
      ___BSS_END__ = .;
    } AT> high

    .data :
    {
        *(.data*)
    } AT> high
    
    ___KERNEL_DATA_END__ = .;
    
    ___VISIBLE_FROM_USERMODE_START__  = .;
    
    .visible_from_usermode 0x8000000000 :
    {
        *(.visible_from_usermode*)
    } AT> high
    
    ___VISIBLE_FROM_USERMODE_END__  = .;
}
