# A bundle is the only thing that gives cydonia a Dock icon. gpui never calls
# `setApplicationIconImage:` — it reads a bundle but does not set one — so the
# icon can come from nothing but `CFBundleIconFile`, and the bare binary
# `cargo install` leaves behind stays generic.
#
# The logo is not in git. It is downloaded once into assets/, which is ignored,
# so a fresh clone needs nothing but the network to build an illustrated app.

VERSION  := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH     := $(shell uname -m)
ICON     := assets/icon.png
ICON_URL := https://cdn.crabtalk.ai/logos/cydonia.png
APP      := target/bundle/cydonia.app
ICONSET  := target/bundle/cydonia.iconset
DMG      := target/bundle/cydonia-$(VERSION)-$(ARCH).dmg
DMGSTAGE := target/bundle/dmg
DMGRW    := target/bundle/cydonia-rw.dmg
DMGMNT   := target/bundle/mnt
# The ten `.icns` wants. Each is also emitted at @2x, so 512 covers 1024.
SIZES    := 16 32 128 256 512
# Ad-hoc, and every target but `release` leaves it that way: a local build signs
# with no certificate and opens on the machine that built it and nowhere else.
# `release` overrides this from .env.release, which is the only place a
# certificate is ever named.
SIGN     ?= -
# 32 bytes of FinderInfo carrying one bit: 0x0400, the custom-icon flag, at the
# flags field a folder keeps eight bytes in. `SetFile -a C` is the usual way and
# wants Xcode; this wants nothing.
CUSTOMICON := 0000000000000000040000000000000000000000000000000000000000000000

.PHONY: bundle dmg release icon open clean

bundle:
	cargo build --release
	rm -rf $(APP) $(ICONSET)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Resources
	cp target/release/cydonia $(APP)/Contents/MacOS/cydonia
	sed 's/@VERSION@/$(VERSION)/g' bundle/Info.plist > $(APP)/Contents/Info.plist
	@# An unreachable CDN costs the app its icon, not its build. The `.icns` is
	@# AppKit's; the 256 png beside it is the app's own — settings paints the
	@# mark itself, and nothing in the app can read an `.icns`.
	@[ -f $(ICON) ] || $(MAKE) --no-print-directory icon || true
	@if [ -f $(ICON) ]; then \
		mkdir -p $(ICONSET); \
		for s in $(SIZES); do \
			sips -s format png -Z $$s $(ICON) --out $(ICONSET)/icon_$${s}x$${s}.png >/dev/null; \
			sips -s format png -Z $$((s * 2)) $(ICON) --out $(ICONSET)/icon_$${s}x$${s}@2x.png >/dev/null; \
		done; \
		iconutil -c icns $(ICONSET) -o $(APP)/Contents/Resources/cydonia.icns; \
		rm -rf $(ICONSET); \
		sips -s format png -Z 256 $(ICON) --out $(APP)/Contents/Resources/icon.png >/dev/null; \
	else \
		echo "no $(ICON): bundling without an icon"; \
	fi
	@# Copying the binary leaves its signature covering the binary alone, and
	@# Gatekeeper refuses a bundle nothing has signed. The hardened runtime and
	@# the timestamp are what notarization will not proceed without; ad-hoc keeps
	@# the first and drops the second rather than failing, so both stay here
	@# unconditionally. No entitlements: agents are spawned, not loaded, and
	@# nothing here is sandboxed.
	codesign --force --options runtime --timestamp --sign "$(SIGN)" $(APP)
	@echo "built $(APP) $(VERSION)"

dmg: bundle
	rm -rf $(DMGSTAGE) $(DMGRW) $(DMG)
	mkdir -p $(DMGSTAGE)
	cp -R $(APP) $(DMGSTAGE)/
	@# Drag-to-install is this symlink and nothing else.
	ln -s /Applications $(DMGSTAGE)/Applications
	@[ -f $(APP)/Contents/Resources/cydonia.icns ] \
		&& cp $(APP)/Contents/Resources/cydonia.icns $(DMGSTAGE)/.VolumeIcon.icns || true
	hdiutil create -volname Cydonia -srcfolder $(DMGSTAGE) -ov -quiet -format UDRW -fs HFS+ $(DMGRW)
	@# `.VolumeIcon.icns` is inert until the volume root carries the custom-icon
	@# bit, and a root exists only once the image is mounted — which is the whole
	@# reason the image is built read-write and compressed afterwards. Detaching
	@# is unconditional: a failure here must not leave the volume mounted.
	@hdiutil attach $(DMGRW) -nobrowse -quiet -mountpoint $(DMGMNT) && \
		{ xattr -wx com.apple.FinderInfo $(CUSTOMICON) $(DMGMNT); hdiutil detach $(DMGMNT) -quiet; }
	@# lzfse over zlib: same second, a tenth off the download. It has been read
	@# since 10.11, well under the bundle's own floor.
	hdiutil convert $(DMGRW) -format ULFO -o $(DMG) -quiet
	rm -rf $(DMGRW) $(DMGSTAGE) $(DMGMNT)
	@# The image is a container rather than code, so it takes the signature but
	@# not the runtime flag.
	codesign --force --timestamp --sign "$(SIGN)" $(DMG)
	@echo "built $(DMG)"

# The one target that reaches for a certificate, and it will not start without
# .env.release naming which. Stapling the image and not the app inside it is the
# ordinary trade: an app dragged out carries no ticket of its own and verifies
# against Apple on first launch, which costs a stapled app one extra round trip.
release:
	@test -f .env.release || { echo "create .env.release: APPLE_SIGNING_IDENTITY, APPLE_KEYCHAIN_PROFILE"; exit 1; }
	@set -a; . ./.env.release; set +a; \
	  : $${APPLE_SIGNING_IDENTITY:?missing in .env.release}; \
	  : $${APPLE_KEYCHAIN_PROFILE:?missing in .env.release}; \
	  set -e; \
	  $(MAKE) --no-print-directory dmg SIGN="$$APPLE_SIGNING_IDENTITY"; \
	  xcrun notarytool submit $(DMG) --keychain-profile "$$APPLE_KEYCHAIN_PROFILE" --wait; \
	  xcrun stapler staple $(DMG); \
	  xcrun stapler validate $(DMG); \
	  spctl -a -vvv -t open --context context:primary-signature $(DMG); \
	  echo "release ready: $(DMG)"

# Also the way to pick up a redrawn logo: it refetches rather than skipping.
# The partial file is named apart from the real one, so an interrupted download
# cannot leave a corrupt icon cached where nothing will look at it again.
icon:
	@mkdir -p $(dir $(ICON))
	curl -fsSL $(ICON_URL) -o $(ICON).part && mv $(ICON).part $(ICON) || { rm -f $(ICON).part; exit 1; }

open: bundle
	open $(APP)

clean:
	rm -rf target/bundle
