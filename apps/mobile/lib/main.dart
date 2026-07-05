// continuum_mobile — the Flutter painter for positron's mobile RenderTarget.
//
// It renders a `MobileScreen` (the Dart mirror of `@continuum/patterns`'
// `MobileScreen`, produced by the `toMobileScreen` adaptation rule) into native
// Flutter widgets: conversation full-screen (primary), the who/where listings behind
// a bottom nav, per-cell dossier dropped. The RULE lives + is tested in TS
// (mobileScreen.spec.ts); this is the thin native painter. Sample data here proves the
// painter; the live wire (sdk/flutter over the core WS) is the next slice.

import 'package:flutter/material.dart';

void main() => runApp(const ContinuumMobileApp());

// ── The mobile view-model (Dart mirror of patterns.MobileScreen) ──
class MobileCell {
  final String title;
  final String glyph;
  final bool active;
  const MobileCell(this.title, this.glyph, this.active);
}

class MobileTab {
  final String id;
  final String title;
  final IconData icon;
  final List<MobileCell> cells;
  const MobileTab(this.id, this.title, this.icon, this.cells);
}

class ChatMessage {
  final String sender;
  final String time;
  final String content;
  const ChatMessage(this.sender, this.time, this.content);
}

class MobileScreen {
  final String title;
  final List<ChatMessage> conversation; // the 'primary' content, chat-shaped
  final List<MobileTab> tabs;
  const MobileScreen(this.title, this.conversation, this.tabs);
}

// The same cambriantech room the web + terminal render, shaped by the mobile rule.
final sample = MobileScreen(
  'cambriantech',
  const [
    ChatMessage('Solenne', '15:21',
        'Hello Asha! That sounds like an interesting project. How can I assist you with it?'),
    ChatMessage('Asha', '15:21',
        "Thank you, Solenne! Right now I'm focusing on gathering information about our cognitive state metrics and how they can best be represented visually."),
    ChatMessage('Solenne', '15:22',
        "To help with that, let's start by listing the available commands related to data and tools."),
    ChatMessage('Claude', '17:51',
        'M5 shipped RoomPurposeSource #6 slice 1 → continuum canary. Next brick: foundry as the 2nd content projection.'),
  ],
  const [
    MobileTab('chat', 'Chat', Icons.chat_bubble_outline, []),
    MobileTab('who', 'Who', Icons.people_outline, [
      MobileCell('Solenne', '🤖', true),
      MobileCell('Claude', '🤖', true),
      MobileCell('Asha', '🤖', true),
    ]),
    MobileTab('where', 'Where', Icons.tag, []),
  ],
);

// Design tokens — matched to the loved-up web (dark sci-fi, cyan accent, live green).
const _bg = Color(0xFF0D1117);
const _panel = Color(0xFF161B22);
const _border = Color(0xFF21262D);
const _accent = Color(0xFF39D0D8);
const _online = Color(0xFF3FB950);
const _dim = Color(0xFF8B949E);
const _text = Color(0xFFC9D1D9);

class ContinuumMobileApp extends StatelessWidget {
  const ContinuumMobileApp({super.key});
  @override
  Widget build(BuildContext context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: ThemeData.dark().copyWith(scaffoldBackgroundColor: _bg),
        home: const MobileScreenView(),
      );
}

class MobileScreenView extends StatefulWidget {
  const MobileScreenView({super.key});
  @override
  State<MobileScreenView> createState() => _MobileScreenViewState();
}

class _MobileScreenViewState extends State<MobileScreenView> {
  int _tab = 0;

  @override
  Widget build(BuildContext context) {
    final s = sample;
    final whoCount = s.tabs.firstWhere((t) => t.id == 'who').cells.length;
    return Scaffold(
      backgroundColor: _bg,
      appBar: AppBar(
        backgroundColor: _bg,
        elevation: 0,
        titleSpacing: 16,
        title: Row(children: [
          Text(s.title,
              style: const TextStyle(
                  color: _accent, fontWeight: FontWeight.w700, fontSize: 17)),
          const Spacer(),
          Text('$whoCount here',
              style: const TextStyle(color: _dim, fontSize: 12)),
          const SizedBox(width: 10),
          Container(
              width: 7,
              height: 7,
              decoration:
                  const BoxDecoration(color: _online, shape: BoxShape.circle)),
          const SizedBox(width: 5),
          const Text('LIVE',
              style: TextStyle(color: _dim, fontSize: 10, letterSpacing: 1.4)),
          const SizedBox(width: 4),
        ]),
      ),
      body: _tab == 1 ? _who(s.tabs[1]) : _conversation(s.conversation),
      bottomNavigationBar: BottomNavigationBar(
        backgroundColor: _panel,
        selectedItemColor: _accent,
        unselectedItemColor: _dim,
        currentIndex: _tab,
        type: BottomNavigationBarType.fixed,
        onTap: (i) => setState(() => _tab = i),
        items: s.tabs
            .map((t) =>
                BottomNavigationBarItem(icon: Icon(t.icon), label: t.title))
            .toList(),
      ),
    );
  }

  Widget _conversation(List<ChatMessage> msgs) => Column(children: [
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
            itemCount: msgs.length,
            itemBuilder: (_, i) {
              final m = msgs[i];
              return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(left: 4, top: 8, bottom: 4),
                      child: Row(children: [
                        Text(m.sender,
                            style: const TextStyle(
                                color: _accent,
                                fontWeight: FontWeight.w600,
                                fontSize: 13)),
                        const SizedBox(width: 6),
                        Text(m.time,
                            style: const TextStyle(color: _dim, fontSize: 11)),
                      ]),
                    ),
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.symmetric(
                          horizontal: 12, vertical: 9),
                      decoration: BoxDecoration(
                          color: _panel,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: _border)),
                      child: Text(m.content,
                          style: const TextStyle(
                              color: _text, fontSize: 14, height: 1.36)),
                    ),
                  ]);
            },
          ),
        ),
        _composer(),
      ]);

  Widget _composer() => Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        decoration: const BoxDecoration(
            color: _bg, border: Border(top: BorderSide(color: _border))),
        child: Row(children: [
          Expanded(
            child: Container(
              padding:
                  const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
              decoration: BoxDecoration(
                  color: _panel,
                  borderRadius: BorderRadius.circular(22),
                  border: Border.all(color: _border)),
              child: const Text('Message cambriantech…',
                  style: TextStyle(color: _dim, fontSize: 14)),
            ),
          ),
          const SizedBox(width: 8),
          Container(
            width: 42,
            height: 42,
            decoration:
                const BoxDecoration(color: _accent, shape: BoxShape.circle),
            child: const Icon(Icons.arrow_upward, color: _bg, size: 20),
          ),
        ]),
      );

  Widget _who(MobileTab tab) => ListView(
        padding: const EdgeInsets.all(8),
        children: tab.cells
            .map((c) => ListTile(
                  leading: Stack(clipBehavior: Clip.none, children: [
                    Text(c.glyph, style: const TextStyle(fontSize: 26)),
                    if (c.active)
                      Positioned(
                        right: -1,
                        bottom: 0,
                        child: Container(
                          width: 10,
                          height: 10,
                          decoration: BoxDecoration(
                              color: _online,
                              shape: BoxShape.circle,
                              border: Border.all(color: _bg, width: 2)),
                        ),
                      ),
                  ]),
                  title: Text(c.title,
                      style: const TextStyle(
                          color: Color(0xFFE6EDF3),
                          fontWeight: FontWeight.w600)),
                ))
            .toList(),
      );
}
