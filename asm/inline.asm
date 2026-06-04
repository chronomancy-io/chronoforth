; +INLINE -INLINE

; Inline threading
; ----------------
; When the outer interpreter compiles a call to one of a curated set of hot,
; position-independent primitives, emit the primitive's machine-code body
; directly into the definition instead of a `jsr WORD` (3 bytes + 12 cycles of
; call/return overhead per occurrence). The primitives themselves are unchanged,
; so internal `jsr`-callers in the kernel pay nothing; only user colon
; definitions get inlined, at compile time.
;
; Safety contract for a word to appear in the table:
;   * its code body ends in a single RTS,
;   * it contains no JSR/JMP and no inline operands (only self-relative
;     branches), so the body is fully position-independent, and
;   * <WORD>_END marks the byte immediately after the body (= the RTS),
;     so the inlined length is <WORD>_END - <WORD>.
; Inlined words are flagged no-TCE (EXIT must append RTS, never rewind into the
; copied body).

il_len  !byte 0
il_y    !byte 0

; Inlining is a controllable optimizer. It is OFF while the kernel compiles the
; system source at boot (keeping the turnkey image compact / cartridge-sized);
; base.fs flips it ON before saving, so interactive & user definitions get
; inlined hot primitives. Toggle at any time with -inline / +inline.
inlining_on !byte 0

    +BACKLINK "+inline", 7
    lda #1
    sta inlining_on
    rts

    +BACKLINK "-inline", 7
    lda #0
    sta inlining_on
    rts

inline_table
    !word DUP
    !byte DUP_END - DUP
    !word OVER
    !byte OVER_END - OVER
    !word SWAP
    !byte SWAP_END - SWAP
    !word PLUS
    !byte PLUS_END - PLUS
    !word MINUS
    !byte MINUS_END - MINUS
    !word ONEPLUS
    !byte ONEPLUS_END - ONEPLUS
    !word ONEMINUS
    !byte ONEMINUS_END - ONEMINUS
    !word INVERT
    !byte INVERT_END - INVERT
    !word EQUAL
    !byte EQUAL_END - EQUAL
    !word ZEQU
    !byte ZEQU_END - ZEQU
    !word NIP
    !byte NIP_END - NIP
    !word ROT
    !byte ROT_END - ROT
    !word FETCH
    !byte FETCH_END - FETCH
    !word STORE
    !byte STORE_END - STORE
    !word LESS_THAN
    !byte LESS_THAN_END - LESS_THAN
    !word TWOSTAR
    !byte TWOSTAR_END - TWOSTAR
    !word GREATER_THAN
    !byte GREATER_THAN_END - GREATER_THAN
    !word TWODUP
    !byte TWODUP_END - TWODUP
    !word TUCK
    !byte TUCK_END - TUCK
    !word MAX
    !byte MAX_END - MAX
    !word MIN
    !byte MIN_END - MIN
    !word NEGATE
    !byte NEGATE_END - NEGATE
    !word ZERO
    !byte ZERO_END - ZERO
    !word ONE
    !byte ONE_END - ONE
    !word MINUS_ONE
    !byte MINUS_ONE_END - MINUS_ONE
    !word BL
    !byte BL_END - BL
    !word S_TO_D
    !byte S_TO_D_END - S_TO_D
    !word NOT_EQUAL
    !byte NOT_EQUAL_END - NOT_EQUAL
    !word ZERO_NOT_EQUAL
    !byte ZERO_NOT_EQUAL_END - ZERO_NOT_EQUAL
    !word U_GREATER
    !byte U_GREATER_END - U_GREATER
inline_table_end

; ( xt -- )  Compile a call to xt: inline its body if it is in the table,
; otherwise fall through to the normal COMPILE, (jsr xt).
compile_xt
    lda inlining_on
    bne +
    jmp COMPILE_COMMA       ; inlining disabled -> normal jsr compile
+
    lda LSB, x
    sta W2
    lda MSB, x
    sta W2 + 1
    ldy #0
.scan
    cpy #(inline_table_end - inline_table)
    bcs .nope
    lda inline_table, y
    cmp W2
    bne .next
    lda inline_table + 1, y
    cmp W2 + 1
    bne .next

    ; match: W2 = body src, length = inline_table+2,y
    lda inline_table + 2, y
    sta il_len
    jsr inline_blit
    ; Inlined => exempt from TCE: the body has no trailing `jsr` for EXIT to
    ; rewrite into a `jmp`. Set CURR (not LAST): the interpreter copies
    ; curr->last when it processes the *next* token (e.g. `;`), and EXIT reads
    ; last. Writing last directly would be clobbered by that copy.
    lda #1
    sta curr_word_no_tail_call_elimination
    inx                     ; drop xt
    rts
.next
    iny
    iny
    iny
    jmp .scan
.nope
    jmp COMPILE_COMMA

; copy il_len bytes from (W2) to HERE, preserving X and the data stack.
inline_blit
    ldy #0
.bloop
    cpy il_len
    beq .bdone
    lda (W2), y
    sty il_y
    jsr compile_a           ; A -> HERE, HERE++, X & stack preserved
    ldy il_y
    iny
    bne .bloop              ; bodies are < 256 bytes
.bdone
    rts
