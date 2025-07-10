ENTRY(start)

MEMORY {
    high : ORIGIN = 25M, LENGTH = 0x700000
}

SECTIONS {
    . = 1M;   /* load kernel at 1 MiB */

    ___KERNEL_DATA_START__ = .;

    .boot :
    {
        *(.multiboot2_header)
    } AT> high

    .text ALIGN (4K) :
    {
        *(.text*)
    } AT> high

   .bss ALIGN (4K) :
    {
      ___BSS_START__ = .;
      *(".bss*")
      ___BSS_END__ = .;
    } AT> high

    .data ALIGN (4K) :
    {
        *(.data*)
    } AT> high
    
    . = 0x8000000000; /* Load user-visible kernel sections at 512GiB */
    
    ___VISIBLE_FROM_USERMODE_START__  = .;
    
    .visible_from_usermode ALIGN (4K) :
    {
        *(.visible_from_usermode*)
    } AT> high
    
    ___VISIBLE_FROM_USERMODE_END__  = .;
    
    ___KERNEL_DATA_END__ = .;
}
