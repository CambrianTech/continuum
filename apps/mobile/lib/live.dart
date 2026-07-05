// live.dart — the Dart mirror of the web SDK's StateConnection. Opens a WebSocket to the
// continuum core, subscribes to kind='chat', and maps each ChatViewState snapshot into a
// MobileScreen. This is what makes the phone a REAL client of the live room, not a static
// demo — the same positron state seam the web + terminal read, now on Android.

import 'dart:convert';

import 'package:flutter/material.dart' show Icons;
import 'package:web_socket_channel/web_socket_channel.dart';

import 'main.dart';

class LiveConnection {
  final String url;
  final void Function(MobileScreen) onScreen;
  WebSocketChannel? _ch;

  LiveConnection(this.url, this.onScreen);

  void connect() {
    try {
      final ch = WebSocketChannel.connect(Uri.parse(url));
      _ch = ch;
      ch.stream.listen(_onMessage, onError: (_) {}, onDone: () {});
      // The subscribe frame — same shape as StateConnection.buildSubscribe().
      ch.sink.add(jsonEncode({
        'type': 'subscribe',
        'kinds': ['chat'],
        'layers': ['ephemeral', 'session', 'persistent', 'semantic'],
        'last_seen': <dynamic>[],
      }));
    } catch (_) {
      // No core reachable — the app stays on its sample data (honest offline).
    }
  }

  void dispose() => _ch?.sink.close();

  void _onMessage(dynamic data) {
    try {
      final msg = jsonDecode(data as String) as Map<String, dynamic>;
      if (msg['type'] != 'state' || msg['kind'] != 'chat') return;
      onScreen(_fromChat(msg['payload'] as Map<String, dynamic>));
    } catch (_) {
      // A malformed frame has no room to render — drop it, keep the last good screen.
    }
  }

  MobileScreen _fromChat(Map<String, dynamic> p) {
    final messages = ((p['messages'] as List?) ?? const []).map((m) {
      final mm = m as Map<String, dynamic>;
      return ChatMessage(
        (mm['sender_name'] as String?) ?? '?',
        _hhmm(mm['timestamp']),
        _text(mm['content']),
      );
    }).toList();
    final who = ((p['roster'] as List?) ?? const []).map((r) {
      final rr = r as Map<String, dynamic>;
      return MobileCell(
        (rr['display_name'] as String?) ?? '?',
        '🤖',
        (rr['active'] as bool?) ?? false,
      );
    }).toList();
    return MobileScreen(
      (p['room_name'] as String?) ?? 'room',
      messages,
      [
        MobileTab('chat', 'Chat', Icons.chat_bubble_outline, const []),
        MobileTab('who', 'Who', Icons.people_outline, who),
        MobileTab('where', 'Where', Icons.tag, const []),
      ],
    );
  }

  // Content may be a plain string or a `{"text":"…"}` envelope — surface the text.
  String _text(dynamic c) {
    if (c is String) {
      if (c.startsWith('{')) {
        try {
          final j = jsonDecode(c);
          if (j is Map && j['text'] is String) return j['text'] as String;
        } catch (_) {/* not JSON — use raw */}
      }
      return c;
    }
    if (c is Map && c['text'] is String) return c['text'] as String;
    return c?.toString() ?? '';
  }

  String _hhmm(dynamic ts) {
    final int ms = ts is int
        ? ts
        : ts is double
            ? ts.toInt()
            : ts is String
                ? (int.tryParse(ts) ?? 0)
                : 0;
    if (ms == 0) return '';
    final d = DateTime.fromMillisecondsSinceEpoch(ms).toLocal();
    return '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';
  }
}
