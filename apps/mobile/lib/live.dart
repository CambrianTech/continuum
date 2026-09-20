// live.dart — the mobile app's wiring of positron's StateConnection (see
// positron_state.dart, the Dart mirror of sdk/typescript). The app owns only
// the MAPPING (ChatViewState → MobileScreen) and the status surface; durability
// (cache-first boot, write-through) and self-healing (reconnect ladder) are
// positron-inherent — this file holds ZERO resilience logic, exactly like the
// web app after the same refactor ([[one-logical-decision-one-place]]).

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart' show Icons;
import 'package:web_socket_channel/web_socket_channel.dart';

import 'main.dart';
import 'positron_state.dart';

/// Real socket over web_socket_channel — the production `StateSocket`.
class _ChannelSocket implements StateSocket {
  final WebSocketChannel _ch;
  _ChannelSocket(this._ch);

  static Future<StateSocket> open(String url) async {
    final ch = WebSocketChannel.connect(Uri.parse(url));
    await ch.ready; // fail-loud connect (the SDK's ladder handles retries)
    return _ChannelSocket(ch);
  }

  @override
  Stream<dynamic> get stream => _ch.stream;

  @override
  void send(String data) => _ch.sink.add(data);

  @override
  Future<void> close() async => _ch.sink.close();
}

class LiveConnection {
  final String url;
  final void Function(MobileScreen) onScreen;

  /// Optional status surface for the app chrome — mirrors the web banner:
  /// `cached` / `connecting` / `live` / `reconnecting` / `closed`.
  final void Function(StateFeedStatus status, String? detail)? onStatus;

  /// Durable cache dir (the app's documents dir on device). Null = in-memory
  /// (still hydrate semantics within the process; no cross-restart persistence).
  final Directory? cacheDir;

  StateConnection? _conn;

  LiveConnection(this.url, this.onScreen, {this.onStatus, this.cacheDir});

  void connect() {
    final conn = StateConnection(
      url,
      socketFactory: _ChannelSocket.open,
      storage: cacheDir != null ? FileStateStorage(cacheDir!) : MemoryStateStorage(),
    );
    _conn = conn;
    if (onStatus != null) conn.onStatus(onStatus!);
    conn.on('chat', (envelope) {
      final p = envelope.payload;
      if (p is Map<String, dynamic>) onScreen(_fromChat(p));
    });
    // Fire-and-forget: with reconnect (the default) connect() self-heals — a
    // dead core means cached/sample data + a `reconnecting` status, never a
    // crash or a silent dead screen.
    unawaited(conn.connect());
  }

  void dispose() => unawaited(_conn?.close() ?? Future<void>.value());

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
