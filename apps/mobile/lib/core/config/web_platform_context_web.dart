// ignore_for_file: avoid_web_libraries_in_flutter, deprecated_member_use

import 'dart:html' as html;

String? currentUserAgent() => html.window.navigator.userAgent;

String? currentHost() => html.window.location.host;

String? currentSearch() => html.window.location.search;

String? currentReferrer() => html.document.referrer;
