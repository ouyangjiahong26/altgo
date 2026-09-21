.PHONY: build ensure-frontend-deps test install clean fmt lint package-deb run

BINARY=altgo
REPO_ROOT := $(abspath .)

# 检查前端依赖是否安装
ensure-frontend-deps:
	@if [ ! -d "$(REPO_ROOT)/frontend/node_modules" ]; then \
		echo "frontend/node_modules not found, running npm install..."; \
		cd "$(REPO_ROOT)/frontend" && npm install; \
	fi

# Release 可执行文件在 src-tauri/target/release/altgo
build: ensure-frontend-deps
	cargo tauri build
	@echo "Run: src-tauri/target/release/altgo (local mode needs the SenseVoice model from Settings)"

test:
	cargo test --manifest-path=src-tauri/Cargo.toml

fmt:
	cargo fmt --manifest-path=src-tauri/Cargo.toml --all -- --check

lint:
	cargo clippy --manifest-path=src-tauri/Cargo.toml --all-targets -- -D warnings

install: build
	install -d $(DESTDIR)/usr/local/bin
	install -m 755 src-tauri/target/release/bundle/deb/*/usr/local/bin/$(BINARY) $(DESTDIR)/usr/local/bin/$(BINARY) 2>/dev/null || \
		install -m 755 src-tauri/target/release/altgo $(DESTDIR)/usr/local/bin/$(BINARY)
	install -d $(DESTDIR)/etc/altgo
	install -m 644 configs/altgo.toml $(DESTDIR)/etc/altgo/altgo.toml

clean:
	cargo clean --manifest-path=src-tauri/Cargo.toml
	rm -f $(BINARY)

# 一键构建并运行
run: build
	$(REPO_ROOT)/src-tauri/target/release/altgo

package-deb: build
	cargo deb --manifest-path=src-tauri/Cargo.toml --no-build
