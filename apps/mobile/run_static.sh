#!/bin/bash
# Launch the Flutter Web PoC with dummy data and no backend dependency.

cd "$(dirname "$0")"

echo "Starting MUSUBI Web PoC on http://localhost:8088 ..."
flutter run -d chrome --web-port=8088 --dart-define=MUSUBI_USE_DUMMY_DATA=true
