; DROP SWAP DUP ?DUP NIP OVER 2DUP 1+ 1- + = 0= <> 0<> AND ! @ C! C@ COUNT < >
; MAX MIN TUCK >R R> R@ BL PICK DEPTH WITHIN ERASE FILL BASE 2* ROT +! SPLIT

    +BACKLINK "drop", 4 | F_IMMEDIATE
DROP
    lda STATE
    bne +
    inx
    rts
+   lda #OP_INX
compile_a
    dex
    sta LSB, x
    jmp CCOMMA

    +BACKLINK "swap", 4
SWAP
    ldy	MSB, x
    lda	MSB + 1, x
    sta MSB, x
    sty	MSB + 1, x

    ldy	LSB, x
    lda	LSB + 1, x
    sta LSB, x
    sty	LSB + 1, x
SWAP_END
    rts

    +BACKLINK "dup", 3
DUP
    dex
    lda	MSB + 1, x
    sta	MSB, x
    lda	LSB + 1, x
    sta	LSB, x
DUP_END
    rts

    +BACKLINK "?dup", 4
QDUP
    lda MSB, x
    ora LSB, x
    bne DUP
    rts

    +BACKLINK "nip", 3
NIP ; ( a b -- b )
    lda LSB, x
    sta LSB+1, x
    lda MSB, x
    sta MSB+1, x
    inx
NIP_END
    rts

    +BACKLINK "over", 4
OVER
    dex
    lda	MSB + 2, x
    sta	MSB, x
    lda	LSB + 2, x
    sta	LSB, x
OVER_END
    rts

    +BACKLINK "2dup", 4
TWODUP ; ( a b -- a b a b )
    dex
    dex
    lda LSB+2, x
    sta LSB, x
    lda MSB+2, x
    sta MSB, x
    lda LSB+3, x
    sta LSB+1, x
    lda MSB+3, x
    sta MSB+1, x
TWODUP_END
    rts

    +BACKLINK "1+", 2
ONEPLUS
    inc LSB, x
    bne +
    inc MSB, x
+
ONEPLUS_END
    rts

    +BACKLINK "1-", 2
ONEMINUS
    lda LSB, x
    bne +
    dec MSB, x
+   dec LSB, x
ONEMINUS_END
    rts

    +BACKLINK "+", 1
PLUS
    lda	LSB, x
    clc
    adc LSB + 1, x
    sta	LSB + 1, x

    lda	MSB, x
    adc MSB + 1, x
    sta MSB + 1, x

    inx
PLUS_END
    rts

    +BACKLINK "=", 1
EQUAL
    ldy #0
    lda	LSB, x
    cmp	LSB + 1, x
    bne	+
    lda	MSB, x
    cmp	MSB + 1, x
    bne	+
    dey
+   inx
    sty MSB, x
    sty	LSB, x
EQUAL_END
    rts

; 0=
    +BACKLINK "0=", 2
ZEQU
    ldy #0
    lda LSB, x
    bne +
    lda MSB, x
    bne +
    dey
+   sty MSB, x
    sty LSB, x
ZEQU_END
    rts

    +BACKLINK "<>", 2
NOT_EQUAL ; ( a b -- flag )  flag = -1 if a<>b else 0
    ldy #$ff
    lda LSB, x
    cmp LSB + 1, x
    bne +
    lda MSB, x
    cmp MSB + 1, x
    bne +
    ldy #0
+   inx
    sty MSB, x
    sty LSB, x
NOT_EQUAL_END
    rts

    +BACKLINK "0<>", 3
ZERO_NOT_EQUAL ; ( n -- flag )  flag = -1 if n<>0 else 0
    ldy #0
    lda LSB, x
    ora MSB, x
    beq +
    ldy #$ff
+   sty MSB, x
    sty LSB, x
ZERO_NOT_EQUAL_END
    rts

    +BACKLINK "and", 3
    lda	MSB, x
    and MSB + 1, x
    sta MSB + 1, x

    lda	LSB, x
    and LSB + 1, x
    sta LSB + 1, x

    inx
    rts

    +BACKLINK "!", 1
STORE
    lda LSB, x
    sta W
    lda MSB, x
    sta W + 1

    ldy #0
    lda	LSB+1, x
    sta (W), y
    iny
    lda	MSB+1, x
    sta	(W), y

    inx
    inx
STORE_END
    rts

    +BACKLINK "@", 1
FETCH
    lda LSB,x
    sta W
    lda MSB,x
    sta W+1

    ldy #0
    lda	(W),y
    sta LSB,x
    iny
    lda	(W),y
    sta MSB,x
FETCH_END
    rts

    +BACKLINK "c!", 2
STOREBYTE
    ldy LSB,x
    lda MSB,x
    sta + + 2
    lda	LSB+1,x
+   sta PLACEHOLDER_ADDRESS,y ; replaced with addr
    inx
    inx
    rts

    +BACKLINK "c@", 2
FETCHBYTE
    ldy LSB,x
    lda MSB,x
    sta + + 2
+   lda PLACEHOLDER_ADDRESS,y ; replaced with addr
    sta LSB,x
    lda #0
    sta MSB,x
    rts

    +BACKLINK "count", 5
COUNT ; ( a -- a+1 c )  c = byte at a
    lda LSB, x
    sta W
    lda MSB, x
    sta W + 1
    inc LSB, x
    bne +
    inc MSB, x
+   dex
    ldy #0
    lda (W), y
    sta LSB, x
    sty MSB, x
    rts

    +BACKLINK "<", 1
LESS_THAN
    ldy #0
    sec
    lda LSB+1,x
    sbc LSB,x
    lda MSB+1,x
    sbc MSB,x
    bvc +
    eor #$80
+   bpl +
    dey
+   inx
    sty LSB,x
    sty MSB,x
LESS_THAN_END
    rts

    +BACKLINK ">", 1
GREATER_THAN ; ( a b -- flag )  flag = a>b = b<a : signed (b - a) < 0
    ldy #0
    sec
    lda LSB,x
    sbc LSB+1,x
    lda MSB,x
    sbc MSB+1,x
    bvc +
    eor #$80
+   bpl +
    dey
+   inx
    sty LSB,x
    sty MSB,x
GREATER_THAN_END
    rts

    +BACKLINK "max", 3
MAX ; ( a b -- max )  signed compare a-b; keep a if a>=b else move b up
    sec
    lda LSB+1, x
    sbc LSB, x
    lda MSB+1, x
    sbc MSB, x
    bvc +
    eor #$80
+   bpl +
    lda LSB, x
    sta LSB+1, x
    lda MSB, x
    sta MSB+1, x
+   inx
MAX_END
    rts

    +BACKLINK "min", 3
MIN ; ( a b -- min )  signed compare a-b; keep a if a<b else move b up
    sec
    lda LSB+1, x
    sbc LSB, x
    lda MSB+1, x
    sbc MSB, x
    bvc +
    eor #$80
+   bmi +
    lda LSB, x
    sta LSB+1, x
    lda MSB, x
    sta MSB+1, x
+   inx
MIN_END
    rts

    +BACKLINK "tuck", 4
TUCK ; ( a b -- b a b )
    dex
    lda LSB+1, x
    sta LSB, x
    lda MSB+1, x
    sta MSB, x
    lda LSB+2, x
    sta LSB+1, x
    lda MSB+2, x
    sta MSB+1, x
    lda LSB, x
    sta LSB+2, x
    lda MSB, x
    sta MSB+2, x
TUCK_END
    rts

    ; Exempt from TCE as top of return stack must contain a return address.
    +BACKLINK ">r", 2 | F_NO_TAIL_CALL_ELIMINATION
TO_R
    pla
    sta W
    pla
    sta W+1
    inc W
    bne +
    inc W+1
+
    lda MSB,x
    pha
    lda LSB,x
    pha
    inx
    jmp (W)

    ; Exempt from TCE as top of return stack must contain a return address.
    +BACKLINK "r>", 2 | F_NO_TAIL_CALL_ELIMINATION
R_TO
    pla
    sta W
    pla
    sta W+1
    inc W
    bne +
    inc W+1
+
    dex
    pla
    sta LSB,x
    pla
    sta MSB,x
    jmp (W)

    ; Exempt from TCE as top of return stack must contain a return address.
    +BACKLINK "r@", 2 | F_NO_TAIL_CALL_ELIMINATION
R_FETCH
    txa
    tsx
    ldy $103,x
    sty W
    ldy $104,x
    tax
    dex
    sty MSB,x
    lda W
    sta LSB,x
    rts

    +BACKLINK "bl", 2
BL
    dex
    lda #K_SPACE
    sta LSB, x
    lda #0
    sta MSB, x
BL_END
    rts

    +BACKLINK "pick", 4
    txa
    sta + + 1
    clc
    adc LSB,x
    tax
    inx
    lda LSB,x
    ldy MSB,x
+   ldx #0
    sta LSB,x
    sty MSB,x
    rts

    +BACKLINK "depth", 5
    txa
    eor #$ff
    tay
    iny
    dex
    sty LSB,x
    lda #0
    sta MSB,x
    rts

    +BACKLINK "within", 6
WITHIN ; ( test low high -- flag )  flag = (test-low) u< (high-low)
    sec                 ; W = high - low
    lda LSB, x
    sbc LSB+1, x
    sta W
    lda MSB, x
    sbc MSB+1, x
    sta W+1
    sec                 ; test - low, in place at x+2
    lda LSB+2, x
    sbc LSB+1, x
    sta LSB+2, x
    lda MSB+2, x
    sbc MSB+1, x
    sta MSB+2, x
    inx                 ; drop low, high; TOS = test-low
    inx
    ldy #0
    lda MSB, x          ; (test-low) u< (high-low) ?
    cmp W+1
    bcc +
    bne ++
    lda LSB, x
    cmp W
    bcs ++
+   dey
++  sty LSB, x
    sty MSB, x
    rts

; ERASE ( start len -- )
    +BACKLINK "erase", 5
    ldy #0
    jmp FILL_Y

; FILL ( start len char -- )
    +BACKLINK "fill", 4
FILL
    lda	LSB, x
    tay
    inx
FILL_Y
    lda	LSB + 1, x
    sta	.fdst
    lda	MSB + 1, x
    sta	.fdst + 1
    lda	LSB, x
    eor	#$ff
    sta	W
    lda	MSB, x
    eor	#$ff
    sta	W + 1
    inx
    inx
-
    inc	W
    bne	+
    inc	W + 1
    bne	+
    rts
+
.fdst = * + 1
    sty	PLACEHOLDER_ADDRESS ; replaced with start

    ; advance
    inc	.fdst
    bne	-
    inc	.fdst + 1
    jmp	-

    +BACKLINK "base", 4
BASE
    +VALUE	_BASE
_BASE
    !word 16

    +BACKLINK "2*", 2
TWOSTAR
    asl LSB, x
    rol MSB, x
TWOSTAR_END
    rts

    +BACKLINK "rot", 3 ; ( a b c -- b c a )
ROT
    ldy MSB+2, x
    lda MSB+1, x
    sta MSB+2, x
    lda MSB  , x
    sta MSB+1, x
    sty MSB  , x
    ldy LSB+2, x
    lda LSB+1, x
    sta LSB+2, x
    lda LSB  , x
    sta LSB+1, x
    sty LSB  , x
ROT_END
    rts

    +BACKLINK "+!", 2 ; ( num addr -- )
PLUS_STORE
    lda LSB,x
    sta W
    lda MSB,x
    sta W+1
    ldy #0
    clc
    lda (W),y
    adc LSB+1,x
    sta (W),y
    iny
    lda (W),y
    adc MSB+1,x
    sta (W),y
    inx
    inx
    rts

    +BACKLINK "split", 5 ; ( n -- lsb msb )
    lda MSB,x
    sta LSB-1,x
    lda #0
    sta MSB,x
    sta MSB-1,x
    dex
    rts
