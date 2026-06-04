C1541 = c1541
AS = acme
# deploy 1571 (d71) or 1581 (d81); e.g. make DISK_SUF=d81 deploy
DISK_SUF = d64

TAG := $(shell git describe --tags --abbrev=0 || svnversion --no-newline)
TAG_DEPLOY_DOT := $(shell git describe --tags --long --dirty=_m | sed 's/-g[0-9a-f]\+//' | tr _- -.)
TAG_DEPLOY := $(shell git describe --tags --abbrev=0 --dirty=_M | tr _. -_)
VERSION_STRING := $(shell git describe --tags --abbrev=0)
GIT_HASH := $(shell git rev-parse --short HEAD)

DEPLOY_NAME = chronoforth-$(TAG_DEPLOY)
DISK_IMAGE = chronoforth.$(DISK_SUF)

X64_DEPLOY_OPTS = -warp -debugcart -limitcycles 2000000000 -reu -reuimage build/reu.ram -reusize 512
X64 = x64sc
PETCAT = petcat # text conversion utility, included in VICE package

SRC_DIR = forth
# Minimal set - removed graphics, sound, demos, float
SRC_NAMES = base debug v asm ls doloop sys labels \
    require compat timer turnkey \
    wordlist io open dos see accept
SRCS = $(addprefix $(SRC_DIR)/,$(addsuffix .fs,$(SRC_NAMES)))

TEST_SRC_NAMES = test testcore testcoreplus testcoreext testexception tester testsee 1
TEST2_SRC_NAMES = see gfx gfxdemo fractals mmldemo mml sid spritedemo sprite compat rnd sin turtle
TEST_SRCS = $(addprefix test/,$(addsuffix .fs,$(TEST_SRC_NAMES)))

SEPARATOR_NAME1 = '=-=-=-=-=-=-=-=,s'
SEPARATOR_NAME2 = '=-------------=,s'
SEPARATOR_NAME3 = '=-=---=-=---=-=,s'

.PHONY: help build test clean docs check all deploy labels emu verify

help:
	@echo "Available targets:"
	@echo "  build    - Build the default disk image ($(DISK_IMAGE))"
	@echo "  test     - Run automated tests (via deploy target)"
	@echo "  clean    - Remove build artifacts"
	@echo "  docs     - Build HTML documentation"
	@echo "  check    - Run disk image in VICE emulator"
	@echo "  all      - Same as build"
	@echo "  deploy   - Full build with tests, cartridge, and PDF manual"
	@echo "  verify   - Cycle/correctness verify via the chrono6502 emulator (fast, headless)"
	@echo ""
	@echo "Toolchain targets:"
	@echo "  $(DISK_IMAGE) - Build disk image directly"

build: $(DISK_IMAGE)

# --- Headless cycle/correctness verification (tools/chrono6502) ---
CHRONO = tools/chrono6502/target/release/chrono6502

labels.vice: durexforth.prg
	$(AS) -I asm --vicelabels labels.vice asm/durexforth.asm

labels: labels.vice

emu:
	cd tools/chrono6502 && cargo build --release

verify: durexforth.prg labels.vice emu
	cd tools/chrono6502 && cargo test
	$(CHRONO) --prg durexforth.prg --labels labels.vice selftest
	$(CHRONO) --prg durexforth.prg --repo . gate

test: deploy

all: $(DISK_IMAGE)

deploy: $(DISK_IMAGE) asm/cart.asm $(TEST_SRCS)
	python asm/header.py $(wildcard asm/*.asm) # verify .asm headers
	rm -rf deploy
	mkdir deploy
	cp $(DISK_IMAGE) deploy/$(DEPLOY_NAME).$(DISK_SUF)
	$(X64) $(X64_DEPLOY_OPTS) -exitscreenshot build/vice-build deploy/$(DEPLOY_NAME).$(DISK_SUF)
	\
	# make test disk
	echo  >build/c1541.script attach deploy/$(DEPLOY_NAME).$(DISK_SUF)
	echo >>build/c1541.script read durexforth
	echo >>build/c1541.script format "test,DF" $(DISK_SUF) deploy/tests.$(DISK_SUF)
	echo >>build/c1541.script write durexforth
	@for forth in $(TEST_SRC_NAMES); do\
		printf aa | cat - test/$$forth.fs | $(PETCAT) -text -w2 -o build/$$forth.pet - ; \
		echo >>build/c1541.script write build/$$forth.pet $$forth; \
	done;
	@for forth in $(TEST2_SRC_NAMES); do\
		printf aa | cat - $(SRC_DIR)/$$forth.fs | $(PETCAT) -text -w2 -o build/$$forth.pet - ; \
		echo >>build/c1541.script write build/$$forth.pet $$forth; \
	done;
	$(C1541) <build/c1541.script
	# run tests
	$(X64) $(X64_DEPLOY_OPTS) -exitscreenshot build/vice-test -keybuf "include test\n" deploy/tests.$(DISK_SUF)
	$(C1541) -attach deploy/tests.$(DISK_SUF) -read ok build/tests_passed
	\
	# make cartridge
	$(C1541) -attach deploy/$(DEPLOY_NAME).$(DISK_SUF) -read durexforth
	mv durexforth build/durexforth
	@$(AS) asm/cart.asm
	cartconv -t simon -i build/cart.bin -o deploy/$(DEPLOY_NAME).crt -n "DUREXFORTH $(TAG_DEPLOY_DOT)"
	asciidoctor-pdf -o deploy/$(DEPLOY_NAME).pdf manual/index.adoc

durexforth.prg: asm/*.asm
	mkdir -p build
	echo >build/version.asm !pet \"CHRONOFORTH $(VERSION_STRING)\"
	@$(AS) -I asm asm/durexforth.asm

$(DISK_IMAGE): durexforth.prg Makefile $(SRCS)
	mkdir -p build
	touch build/empty
	echo  >build/c1541.script format "chronoforth,CF" $(DISK_SUF) $@
	echo >>build/c1541.script write durexforth.prg durexforth
	echo >>build/c1541.script write build/empty $(SEPARATOR_NAME1)
	echo >>build/c1541.script write build/empty $(TAG_DEPLOY_DOT),s
	echo >>build/c1541.script write build/empty '  '$(GIT_HASH),s
	echo >>build/c1541.script write build/empty $(SEPARATOR_NAME2)
	@for forth in $(SRC_NAMES); do\
		printf aa | cat - $(SRC_DIR)/$$forth.fs | $(PETCAT) -text -w2 -o build/$$forth.pet - ; \
		echo >>build/c1541.script write build/$$forth.pet $$forth; \
	done;
	echo >>build/c1541.script write build/empty $(SEPARATOR_NAME3)
	$(C1541) <build/c1541.script

docs: docs/index.html

docs/index.html: manual/index.adoc manual/words.adoc manual/links.adoc manual/sid.adoc manual/asm.adoc \
	manual/mnemonics.adoc manual/memmap.adoc manual/anatomy.adoc LICENSE.txt manual/tutorial.adoc \
	manual/intro.adoc manual/exceptions.adoc
	rm -rf docs
	asciidoctor -a revnumber=$(shell git describe --tags --dirty) -a revdate=$(shell git log -1 --format=%as) -o docs/index.html manual/index.adoc

check: $(DISK_IMAGE)
	$(X64) $(DISK_IMAGE)

clean:
	rm -f *.lbl *.prg *.$(DISK_SUF) labels.vice
	rm -rf build deploy
