ENTRY(start)

SECTIONS {
    . = 1M;   /* load kernel at 1 MiB */

    ___KERNEL_DATA_START__ = .;

    .boot :
    {
        *(.multiboot2_header)
    }

    .text ALIGN (4K) :
    {
        *(.text*)
    }

   .bss ALIGN (4K) :
    {
      ___BSS_START__ = .;
      *(".bss*")
      ___BSS_END__ = .;
    }

    .data ALIGN (4K) :
    {
        *(.data*)
    }
    
    . += 4K;
    
    . = ALIGN (4K);
    
    ___VISIBLE_FROM_USERMODE_START__  = .;
    
    .visible_from_usermode :
    {
        *(.visible_from_usermode*)
    }
    
    ___VISIBLE_FROM_USERMODE_END__  = .;
    
    ___KERNEL_DATA_END__ = .;
}
