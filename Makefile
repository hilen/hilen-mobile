
include build/common.mk

template:
	open ./mobile-template/iOS/HILEN_MOBILE_PROJECT_NAME_CAMEL_CASE.xcodeproj

ios:
	./build/ios/build-project.sh --path=./mobile-template
