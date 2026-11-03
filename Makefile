UNAME_S := $(shell uname -s)
CODESIGN_CERT ?= favilo-cert
CODESIGN_ID := com.favilo.penrose

define sign_macos_binary
	@KEYCHAIN="$$HOME/Library/Keychains/login.keychain-db"; \
	if security find-identity -p codesigning "$$KEYCHAIN" 2>/dev/null | grep -q "$(CODESIGN_CERT)"; then \
		echo ":: Signing $(1) with certificate '$(CODESIGN_CERT)'..."; \
		codesign --force --identifier "$(CODESIGN_ID)" --keychain "$$KEYCHAIN" --sign "$(CODESIGN_CERT)" "$(1)"; \
	else \
		echo ":: Warning: certificate '$(CODESIGN_CERT)' not found in $$KEYCHAIN; falling back to ad-hoc signature."; \
		codesign --force --identifier "$(CODESIGN_ID)" --sign - "$(1)"; \
	fi
endef

all: build

.PHONY: build
build:
	@if [ "$$(id -u)" -eq 0 ]; then echo "build can not be run as root"; exit 1; fi
	@echo ":: Rebuilding in release mode..."
	@cargo build --release
ifeq ($(UNAME_S),Darwin)
	$(call sign_macos_binary,target/release/favilo-penrose)
endif


.PHONY: build-debug
build-debug:
	@if [ "$$(id -u)" -eq 0 ]; then echo "build can not be run as root"; exit 1; fi
	@echo ":: Rebuilding in debug mode..."
	@cargo build
ifeq ($(UNAME_S),Darwin)
	@strip target/debug/favilo-penrose
	$(call sign_macos_binary,target/debug/favilo-penrose)
endif

.PHONY: macos-trust-cert
macos-trust-cert:
	@USER_HOME=$$(eval echo ~$${SUDO_USER:-$$USER}); \
	KEYCHAIN="$$USER_HOME/Library/Keychains/login.keychain-db"; \
	TMP_CERT=$$(mktemp /tmp/favilo-cert.XXXXXX.pem); \
	if ! security find-certificate -c "$(CODESIGN_CERT)" -p "$$KEYCHAIN" > "$$TMP_CERT" 2>/dev/null || [ ! -s "$$TMP_CERT" ]; then \
		rm -f "$$TMP_CERT"; \
		echo ":: Error: certificate '$(CODESIGN_CERT)' not found in $$KEYCHAIN"; \
		exit 1; \
	fi; \
	echo ":: Marking '$(CODESIGN_CERT)' as trusted for Code Signing in System.keychain..."; \
	sudo security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain "$$TMP_CERT"; \
	rm -f "$$TMP_CERT"; \
	echo ":: Done trusting '$(CODESIGN_CERT)'"

.PHONY: macos-unbind-keys
macos-unbind-keys:
	@echo ":: Disabling macOS screenshot shortcuts (Cmd+Shift+3/4/5)..."
	@RUN_DEFAULTS="defaults"; \
	if [ -n "$$SUDO_USER" ]; then RUN_DEFAULTS="sudo -u $$SUDO_USER defaults"; fi; \
	for id in 28 29 30 31 184; do \
		$$RUN_DEFAULTS write com.apple.symbolichotkeys AppleSymbolicHotKeys -dict-add "$$id" "<dict><key>enabled</key><false/></dict>"; \
	done; \
	echo ":: Rebinding Firefox & Chrome menu shortcuts to Ctrl..."; \
	$$RUN_DEFAULTS write org.mozilla.firefox NSUserKeyEquivalents -dict-add \
		"New Tab" "^t" \
		"New Window" "^n" \
		"New Private Window" "^$$p" \
		"Reopen Closed Tab" "^$$t" \
		"Open File…" "^o" \
		"Open Location…" "^l" \
		"Close Tab" "^w" \
		"Close Window" "^$$w" \
		"Save Page As…" "^s" \
		"Print…" "^p" \
		"Reload" "^r" \
		"Web Search…" "^k"; \
	$$RUN_DEFAULTS write com.google.Chrome NSUserKeyEquivalents -dict-add \
		"New Tab" "^t" \
		"New Window" "^n" \
		"New Incognito Window" "^$$n" \
		"Reopen Closed Tab" "^$$t" \
		"Open File…" "^o" \
		"Open Location…" "^l" \
		"Close Tab" "^w" \
		"Close Window" "^$$w" \
		"Close All" "^$$w" \
		"Save Page As…" "^s" \
		"Print…" "^p" \
		"Reload This Page" "^r" \
		"Force Reload This Page" "^$$r"; \
	$$RUN_DEFAULTS write com.apple.universalaccess com.apple.custommenu.apps -array-add \
		"org.mozilla.firefox" \
		"com.google.Chrome"; \
	if [ -n "$$SUDO_USER" ]; then \
		sudo -u "$$SUDO_USER" /System/Library/PrivateFrameworks/SystemAdministration.framework/Resources/activateSettings -u; \
	else \
		/System/Library/PrivateFrameworks/SystemAdministration.framework/Resources/activateSettings -u; \
	fi
	@echo ":: Done configuring macOS shortcuts"

.PHONY: install-helpers
install-helpers:
ifeq ($(UNAME_S),Darwin)
	@echo ":: Installing LaunchAgent..."
	@USER_HOME=$$(eval echo ~$${SUDO_USER:-$$USER}); \
	mkdir -p "$$USER_HOME/Library/LaunchAgents"; \
	cp -f services/launchd/com.favilo.penrose.plist "$$USER_HOME/Library/LaunchAgents/"; \
	if [ -n "$$SUDO_USER" ]; then chown "$$SUDO_USER" "$$USER_HOME/Library/LaunchAgents/com.favilo.penrose.plist"; fi
	@echo ":: Configuring macOS Spaces settings..."
	@if [ -n "$$SUDO_USER" ]; then \
		sudo -u "$$SUDO_USER" defaults write com.apple.dock mru-spaces -bool false; \
		sudo -u "$$SUDO_USER" defaults write com.apple.spaces spans-displays -bool false; \
	else \
		defaults write com.apple.dock mru-spaces -bool false; \
		defaults write com.apple.spaces spans-displays -bool false; \
	fi
	@$(MAKE) macos-unbind-keys
	@echo ":: Remember to grant Accessibility permission to /usr/local/bin/favilo-penrose"
	@echo ":: in System Settings -> Privacy & Security -> Accessibility"
	@echo ":: Run 'launchctl load ~/Library/LaunchAgents/com.favilo.penrose.plist' to start the service"
else
	@echo ":: Installing ./bin..."
	@mkdir -p /usr/local/bin
	@cp -R bin/. /usr/local/bin
	@ls bin | xargs -I {} chmod 755 /usr/local/bin/{}
	@echo ":: Installing ./scripts..."
	@cp -r scripts /usr/local
	@echo ":: Installing .service files"
	@cp -r services/systemd/user/* /usr/lib/systemd/user/
	@cp -r services/systemd/system/* /usr/lib/systemd/system/
	@ls scripts | xargs -I {} chmod 755 /usr/local/scripts/{}
	@echo ":: Copying over xsession file..."
	@cp penrose.desktop /usr/share/xsessions/
	# @systemctl --user daemon-reload
	@echo ":: Remember to run 'systemctl --user daemon-reload'"
	@echo "::             and 'sudo systemctl daemon-reload'"
	@echo ":: in order to reload the systemd services"
endif

.PHONY: install-penrose-release
install-penrose-release:
	@echo ":: Installing release build of favilo-penrose..."
	@mkdir -p /usr/local/bin
	@cp -f target/release/favilo-penrose /usr/local/bin
	@chmod 755 /usr/local/bin/favilo-penrose

.PHONY: install-penrose-debug
install-penrose-debug:
	@echo ":: Installing debug build of favilo-penrose..."
	@mkdir -p /usr/local/bin
	@cp -f target/debug/favilo-penrose /usr/local/bin
	@chmod 755 /usr/local/bin/favilo-penrose

.PHONY: install
install: install-penrose-release install-helpers
	@echo ":: Done"
	
.PHONY: install-debug
install-debug: install-penrose-debug install-helpers
	@echo ":: Done"

.PHONY: uninstall
uninstall:
	@echo ":: Removing binaries..."
	@rm -f /usr/local/bin/favilo-penrose
ifeq ($(UNAME_S),Darwin)
	@USER_HOME=$$(eval echo ~$${SUDO_USER:-$$USER}); \
	rm -f "$$USER_HOME/Library/LaunchAgents/com.favilo.penrose.plist"
else
	@ls bin | xargs -I {} rm -f /usr/local/bin/{}
	@echo ":: Removing scripts..."
	@ls scripts | xargs -I {} rm -f /usr/local/scripts/{}
endif
	@echo ":: Done"

.PHONY: update-penrose
update-penrose:
	@echo "Updating to latest version of penrose from GitHub..."
	cargo update -p penrose
