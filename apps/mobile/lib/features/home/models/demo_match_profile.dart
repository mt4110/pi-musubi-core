class DemoMatchProfile {
  const DemoMatchProfile({
    required this.id,
    required this.piUid,
    required this.name,
    required this.age,
    required this.city,
    required this.photoUrl,
    required this.headline,
    required this.bio,
    required this.hobbies,
    required this.intent,
  });

  final String id;
  final String piUid;
  final String name;
  final int age;
  final String city;
  final String photoUrl;
  final String headline;
  final String bio;
  final List<String> hobbies;
  final String intent;
}

const demoMatchProfiles = <DemoMatchProfile>[
  DemoMatchProfile(
    id: 'mai',
    piUid: 'pi-mai-serious-01',
    name: 'Mai',
    age: 27,
    city: 'Tokyo',
    photoUrl: 'https://i.pravatar.cc/900?img=47',
    headline: '静かな夜に、ちゃんと会話できる人が好き。',
    bio: '平日はプロダクトデザイナー。休日は小さな喫茶店と、散歩の延長みたいなデートが理想です。',
    hobbies: ['Coffee', 'Gallery', 'Morning Walk'],
    intent: '2週間以内に実際に会える人だけ探しています。',
  ),
  DemoMatchProfile(
    id: 'ren',
    piUid: 'pi-ren-serious-02',
    name: 'Ren',
    age: 30,
    city: 'Yokohama',
    photoUrl: 'https://i.pravatar.cc/900?img=12',
    headline: '会う前に、礼儀とテンポが合うかを大事にしたい。',
    bio: '営業帰りに海沿いを歩くのが習慣。軽いやり取りより、短くても温度のある会話が好きです。',
    hobbies: ['Seaside', 'Wine', 'Architecture'],
    intent: 'ドタキャンなしで、丁寧に予定を決められる相手を希望。',
  ),
  DemoMatchProfile(
    id: 'yui',
    piUid: 'pi-yui-serious-03',
    name: 'Yui',
    age: 25,
    city: 'Kyoto',
    photoUrl: 'https://i.pravatar.cc/900?img=32',
    headline: 'やわらかい空気感でも、約束はきちんと守りたい。',
    bio: '書店員。古い映画とレコードが好きです。派手さよりも、落ち着いて続く関係を探しています。',
    hobbies: ['Bookstore', 'Cinema', 'Records'],
    intent: '今月中に一度は会える前提でつながりたいです。',
  ),
  DemoMatchProfile(
    id: 'sota',
    piUid: 'pi-sota-serious-04',
    name: 'Sota',
    age: 29,
    city: 'Osaka',
    photoUrl: 'https://i.pravatar.cc/900?img=15',
    headline: '仕事も恋愛も、ふわっとさせないタイプです。',
    bio: 'スタートアップで事業開発。最初の一回を気持ちよく実現するための、段取りの良さには自信があります。',
    hobbies: ['Running', 'Sushi', 'Podcasts'],
    intent: '本気の人だけ、短いメッセージから始めましょう。',
  ),
];

DemoMatchProfile? findDemoMatchProfileById(String id) {
  for (final profile in demoMatchProfiles) {
    if (profile.id == id) {
      return profile;
    }
  }
  return null;
}
