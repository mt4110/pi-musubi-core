class PiAuthSession {
  const PiAuthSession({
    required this.userId,
    required this.piUid,
    required this.displayName,
  });

  final String userId;
  final String piUid;
  final String displayName;
}
