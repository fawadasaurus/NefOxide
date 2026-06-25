SDK_ROOT := ../../lib/NikonSDK
BUILD_DIR := build
PRODUCT := $(BUILD_DIR)/nefoxide-render

CXX := clang++
CXXFLAGS := -std=c++17 -ObjC++ -fobjc-arc -Wall -Wextra -I$(SDK_ROOT)/Include -DNEFOXIDE_REPO_ROOT=\"$(abspath ../..)\"
LDFLAGS := -framework Foundation -framework CoreGraphics -framework ImageIO -Wl,-rpath,@executable_path/../../../lib/NikonSDK/Frameworks

.PHONY: all run clean prepare-resources

all: $(PRODUCT) prepare-resources

$(PRODUCT): main.mm
	mkdir -p $(BUILD_DIR)
	$(CXX) $(CXXFLAGS) $< -o $@ $(LDFLAGS)

prepare-resources:
	mkdir -p $(BUILD_DIR)/Contents/Resources
	cp -R $(SDK_ROOT)/Resources/* $(BUILD_DIR)/Contents/Resources/ 2>/dev/null || true

run: all
	@if [ -z "$(INPUT)" ] || [ -z "$(OUTPUT)" ]; then \
		echo "usage: make run INPUT=/path/to/input.nef OUTPUT=/path/to/output.jpg"; \
		exit 2; \
	fi
	$(PRODUCT) "$(INPUT)" "$(OUTPUT)"

clean:
	rm -rf $(BUILD_DIR)