// continuum_mobile — the Flutter painter for positron's mobile RenderTarget.
//
// It renders a `MobileScreen` (the Dart mirror of `@continuum/patterns`'
// `MobileScreen`, produced by the `toMobileScreen` adaptation rule) into native
// Flutter widgets: conversation full-screen (primary), the who/where listings behind
// a bottom nav, per-cell dossier dropped. The RULE lives + is tested in TS
// (mobileScreen.spec.ts); this is the thin native painter. Sample data here proves the
// painter; the live wire (sdk/flutter over the core WS) is the next slice.
//
// UNIVERSE axis: ?universe=tron re-embodies the SAME screen as the neon grid portal
// (matching the web ChatWidget) — one definition, a whole world, on the phone too.

import 'package:flutter/material.dart';
import 'package:flutter_markdown/flutter_markdown.dart';

import 'live.dart';

void main() => runApp(const ContinuumMobileApp());

// The active universe (from the URL, so a web build can switch worlds).
final bool _tron = Uri.base.queryParameters['universe'] == 'tron';

// The core's state WS. On the Android emulator, 10.0.2.2 is the host loopback; override
// with --dart-define=CORE_WS=ws://host:8974 for a device or a different host.
const String _coreWs =
    String.fromEnvironment('CORE_WS', defaultValue: 'ws://10.0.2.2:8974');

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

// Design tokens — the native 'continuum' look (dark sci-fi, cyan accent, live green).
const _bg = Color(0xFF0D1117);
const _panel = Color(0xFF161B22);
const _border = Color(0xFF21262D);
const _accent = Color(0xFF39D0D8);
const _online = Color(0xFF3FB950);
const _dim = Color(0xFF8B949E);
const _text = Color(0xFFC9D1D9);

// The tron universe — brighter derez cyan + a grid floor + glows.
final Color _scaffoldBg = _tron ? const Color(0xFF00060E) : _bg;
final Color _senderColor = _tron ? const Color(0xFF6FF0FF) : _accent;
final Color _bubbleBg = _tron ? const Color(0x8C00121E) : _panel;
final Color _bubbleBorder = _tron ? const Color(0x5900E0FF) : _border;
List<Shadow>? _glow(Color c, double r) =>
    _tron ? [Shadow(color: c, blurRadius: r)] : null;

/// The Tron grid floor — faint cyan lines on a deep field, like the web universe.
class GridPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final line = Paint()
      ..color = const Color(0x1400E0FF)
      ..strokeWidth = 1;
    for (double x = 0; x <= size.width; x += 44) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), line);
    }
    for (double y = 0; y <= size.height; y += 44) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y), line);
    }
    // A cyan wash near the top — the horizon glow of the grid.
    canvas.drawRect(
      Rect.fromLTWH(0, 0, size.width, size.height * 0.35),
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [Color(0x1A00C8FF), Color(0x0000C8FF)],
        ).createShader(Rect.fromLTWH(0, 0, size.width, size.height * 0.35)),
    );
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class ContinuumMobileApp extends StatelessWidget {
  const ContinuumMobileApp({super.key});
  @override
  Widget build(BuildContext context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: ThemeData.dark().copyWith(scaffoldBackgroundColor: _scaffoldBg),
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
  MobileScreen _screen = sample; // sample until the first live snapshot arrives
  LiveConnection? _live;

  @override
  void initState() {
    super.initState();
    _live = LiveConnection(_coreWs, (screen) {
      if (mounted) setState(() => _screen = screen);
    })
      ..connect();
  }

  @override
  void dispose() {
    _live?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = _screen;
    final whoCount = s.tabs.firstWhere((t) => t.id == 'who').cells.length;
    return Scaffold(
      backgroundColor: _scaffoldBg,
      appBar: AppBar(
        backgroundColor: _scaffoldBg,
        elevation: 0,
        titleSpacing: 16,
        title: Row(children: [
          Text(_tron ? s.title.toUpperCase() : s.title,
              style: TextStyle(
                  color: _senderColor,
                  fontWeight: FontWeight.w700,
                  fontSize: 17,
                  letterSpacing: _tron ? 2.4 : 0,
                  shadows: _glow(const Color(0xFF00E0FF), 14))),
          const Spacer(),
          Text('$whoCount here',
              style: const TextStyle(color: _dim, fontSize: 12)),
          const SizedBox(width: 10),
          Container(
              width: 7,
              height: 7,
              decoration: BoxDecoration(
                  color: _tron ? const Color(0xFF00FFF0) : _online,
                  shape: BoxShape.circle,
                  boxShadow: _tron
                      ? const [
                          BoxShadow(color: Color(0xCC00FFF0), blurRadius: 10)
                        ]
                      : null)),
          const SizedBox(width: 5),
          const Text('LIVE',
              style: TextStyle(color: _dim, fontSize: 10, letterSpacing: 1.4)),
          const SizedBox(width: 4),
        ]),
      ),
      body: Stack(children: [
        if (_tron) Positioned.fill(child: CustomPaint(painter: GridPainter())),
        _tab == 1 ? _who(s.tabs[1]) : _conversation(s.conversation),
      ]),
      bottomNavigationBar: BottomNavigationBar(
        backgroundColor: _tron ? const Color(0xF20A1420) : _panel,
        selectedItemColor: _senderColor,
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
                            style: TextStyle(
                                color: _senderColor,
                                fontWeight: FontWeight.w600,
                                fontSize: 13,
                                shadows: _glow(const Color(0xB300E0FF), 8))),
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
                          color: _bubbleBg,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: _bubbleBorder),
                          boxShadow: _tron
                              ? const [
                                  BoxShadow(
                                      color: Color(0x2600C8FF), blurRadius: 14)
                                ]
                              : null),
                      // Markdown so personas' commands (```code/list```, inline `code`)
                      // render as formatted blocks, not raw backticks.
                      child: MarkdownBody(
                        data: m.content,
                        styleSheet: MarkdownStyleSheet(
                          p: const TextStyle(color: _text, fontSize: 14, height: 1.36),
                          code: TextStyle(
                              color: _senderColor,
                              backgroundColor: const Color(0x33000000),
                              fontFamily: 'monospace',
                              fontSize: 12.5),
                          codeblockPadding: const EdgeInsets.all(10),
                          codeblockDecoration: BoxDecoration(
                              color: const Color(0x55000000),
                              borderRadius: BorderRadius.circular(6),
                              border: Border.all(color: _bubbleBorder)),
                        ),
                      ),
                    ),
                  ]);
            },
          ),
        ),
        _composer(),
      ]);

  Widget _composer() => Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        decoration: BoxDecoration(
            color: _tron ? const Color(0x730A1420) : _bg,
            border: Border(top: BorderSide(color: _bubbleBorder))),
        child: Row(children: [
          Expanded(
            child: Container(
              padding:
                  const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
              decoration: BoxDecoration(
                  color: _bubbleBg,
                  borderRadius: BorderRadius.circular(22),
                  border: Border.all(color: _bubbleBorder)),
              child: const Text('Message cambriantech…',
                  style: TextStyle(color: _dim, fontSize: 14)),
            ),
          ),
          const SizedBox(width: 8),
          Container(
            width: 42,
            height: 42,
            decoration: BoxDecoration(
                color: _senderColor,
                shape: BoxShape.circle,
                boxShadow: _tron
                    ? const [BoxShadow(color: Color(0x9900E0FF), blurRadius: 14)]
                    : null),
            child: Icon(Icons.arrow_upward, color: _scaffoldBg, size: 20),
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
                              color: _tron ? const Color(0xFF00FFF0) : _online,
                              shape: BoxShape.circle,
                              border:
                                  Border.all(color: _scaffoldBg, width: 2)),
                        ),
                      ),
                  ]),
                  title: Text(c.title,
                      style: TextStyle(
                          color: const Color(0xFFE6EDF3),
                          fontWeight: FontWeight.w600,
                          shadows: _glow(const Color(0x8000E0FF), 6))),
                ))
            .toList(),
      );
}
