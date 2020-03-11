.global context_save
context_save:
    // save caller saved registers
    stp x0, x1,   [SP, #-32]!
    stp x2, x3,   [SP, #-32]!
    stp x4, x5,   [SP, #-32]!
    stp x6, x7,   [SP, #-32]!
    stp x8, x9,   [SP, #-32]!
    stp x10, x11, [SP, #-32]!
    stp x12, x13, [SP, #-32]!
    stp x14, x15, [SP, #-32]!
    stp x16, x17, [SP, #-32]!
    stp x18, x30, [SP, #-32]! // don't forget to save the lr (x30)

    // pass the parameters
    mov x0, x29     // info struct (set up by HANDLER macro)
    mrs x1, ESR_EL1 // syndrome
    mov x2, xzr     // 0 (place holder for trap frame param)

    bl handle_exception

.global context_restore
context_restore:
    // FIXME: Restore the context from the stack.
    ret

.macro HANDLER source, kind
    .align 7
    stp     lr, xzr, [SP, #-16]!
    stp     x28, x29, [SP, #-16]!
    
    mov     x29, \source
    movk    x29, \kind, LSL #16
    bl      context_save
    
    ldp     x28, x29, [SP], #16
    ldp     lr, xzr, [SP], #16
    eret
.endm
    
.align 11
.global vectors
vectors:
    // current el w sp0
    HANDLER 0, 0 //synchronous
    HANDLER 0, 1 //irq/virq
    HANDLER 0, 2 //fiq/vfiq
    HANDLER 0, 3 //serror/vserror

    // current el w spx
    HANDLER 1, 0 //synchronous
    HANDLER 1, 1 //irq/virq
    HANDLER 1, 2 //fiq/vfiq
    HANDLER 1, 3 //serror/vserror

    // lower el 64 
    HANDLER 2, 0 //synchronous
    HANDLER 2, 1 //irq/virq
    HANDLER 2, 2 //fiq/vfiq
    HANDLER 2, 3 //serror/vserror

    // lower el 32 
    HANDLER 3, 0 //synchronous
    HANDLER 3, 1 //irq/virq
    HANDLER 3, 2 //fiq/vfiq
    HANDLER 3, 3 //serror/vserror