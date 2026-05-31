.PHONY: all format check serve

APP_FILES := index.html style.css app.js sw.js icon.svg manifest.json
PRETTIER_FILES := index.html style.css app.js sw.js manifest.json README.md INSTRUCTIONS.md eslint.config.mjs

all: format check

format:
	@command -v sponge >/dev/null 2>&1 || { printf 'sponge is required. Install moreutils.\n'; exit 1; }
	python3 -m json.tool --indent 2 manifest.json | sponge manifest.json
	npx prettier --write $(PRETTIER_FILES)
	npx prettier --write --parser html icon.svg

check:
	@missing=0; for file in $(APP_FILES); do if [ ! -f "$$file" ]; then printf 'Missing required file: %s\n' "$$file"; missing=1; fi; done; exit $$missing
	python3 -m json.tool manifest.json >/dev/null
	npx prettier --check $(PRETTIER_FILES)
	npx prettier --check --parser html icon.svg
	npx eslint app.js sw.js
	node --check app.js
	node --check sw.js
	@for file in $(APP_FILES); do grep -q "\"./$$file\"" sw.js || { printf 'sw.js does not list %s\n' "$$file"; exit 1; }; done
	@grep -q '"./"' sw.js || { printf 'sw.js does not list ./\n'; exit 1; }

serve:
	python3 -m http.server 8000
