.PHONY: pub-get analyze run-web build-web

pub-get:
	mise exec -- flutter pub get

analyze:
	mise exec -- flutter analyze

run-web:
	mise exec -- flutter run -d chrome --dart-define=MUSUBI_USE_DUMMY_DATA=false

build-web:
	mise exec -- flutter build web --release --wasm
