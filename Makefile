UNAME_S := $(shell uname -s)

all: build

.PHONY: build
build:
	@if [ "$$(id -u)" -eq 0 ]; then echo "build can not be run as root"; exit 1; fi
	@echo ":: Rebuilding in release mode..."
	@cargo build --release


.PHONY: build-debug
build-debug:
	@if [ "$$(id -u)" -eq 0 ]; then echo "build can not be run as root"; exit 1; fi
	@echo ":: Rebuilding in debug mode..."
	@cargo build

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
ifeq ($(UNAME_S),Darwin)
	@codesign --force --sign - /usr/local/bin/favilo-penrose
endif

.PHONY: install-penrose-debug
install-penrose-debug:
	@echo ":: Installing debug build of favilo-penrose..."
	@strip target/debug/favilo-penrose
	@mkdir -p /usr/local/bin
	@cp -f target/debug/favilo-penrose /usr/local/bin
	@chmod 755 /usr/local/bin/favilo-penrose
ifeq ($(UNAME_S),Darwin)
	@codesign --force --sign - /usr/local/bin/favilo-penrose
endif

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
