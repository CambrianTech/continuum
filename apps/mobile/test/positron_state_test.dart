// positron_state_test.dart — mobile mirror of sdk/typescript/StateConnection.spec.ts.
// Pure Dart (no flutter imports): these pins prove the SAME durable-state
// contract on the code that ships to BOTH Android and iOS.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:continuum_mobile/positron_state.dart';

/// Scriptable socket — sibling of the TS spec's FakeWebSocket.
class FakeSocket implements StateSocket {
  final _controller = StreamController<dynamic>.broadcast();
  final List<String> sent = [];
  bool closed = false;

  @override
  Stream<dynamic> get stream => _controller.stream;

  @override
  void send(String data) => sent.add(data);

  @override
  Future<void> close() async {
    closed = true;
    await _controller.close();
  }

  // test-side triggers
  void deliver(Map<String, dynamic> frame) => _controller.add(jsonEncode(frame));
  Future<void> drop() async => _controller.close(); // core reboot: onDone fires
}

Map<String, dynamic> stateFrame(String kind, int? revision, dynamic payload) => {
      'type': 'state',
      'kind': kind,
      'revision': revision,
      'layer': 'ephemeral',
      'payload': payload,
    };

void main() {
  group('StateConnection (positron durable-state contract)', () {
    // what this catches: the Twitter model — cached envelopes must paint BEFORE
    // the socket opens (instant last-known UI, even against a dead core), under
    // a `cached` status.
    test('hydrates cached envelopes to sinks before the socket opens', () async {
      final storage = MemoryStateStorage();
      await storage.save(
          'scopeA',
          const StateEnvelope(
              kind: 'chat', revision: 7, layer: 'ephemeral', payload: {'cached': true}));
      final socketOpened = <bool>[];
      final seen = <StateEnvelope>[];
      final statuses = <StateFeedStatus>[];
      final conn = StateConnection(
        'ws://x',
        socketFactory: (_) async {
          socketOpened.add(true);
          return FakeSocket();
        },
        storage: storage,
        scope: 'scopeA',
      );
      conn.onStatus((s, _) => statuses.add(s));
      conn.on('chat', (e) {
        if (socketOpened.isEmpty) seen.add(e); // must arrive PRE-socket
      });
      await conn.connect();
      expect(seen, hasLength(1));
      expect((seen.first.payload as Map)['cached'], isTrue);
      expect(statuses, contains(StateFeedStatus.cached));
      await conn.close();
    });

    // what this catches: write-through — every live envelope must land in the
    // adapter so the NEXT boot hydrates the newest snapshot; a feed that renders
    // but forgets makes the cache silently stale.
    test('writes each live envelope through to storage', () async {
      final storage = MemoryStateStorage();
      final socket = FakeSocket();
      final conn = StateConnection(
        'ws://x',
        socketFactory: (_) async => socket,
        storage: storage,
        scope: 'scopeB',
      );
      conn.on('chat', (_) {});
      await conn.connect();
      socket.deliver(stateFrame('chat', 3, {'live': true}));
      await Future<void>.delayed(Duration.zero); // let stream + save settle
      final rows = await storage.load('scopeB');
      expect(rows, hasLength(1));
      expect((rows.first.envelope.payload as Map)['live'], isTrue);
      await conn.close();
    });

    // what this catches: a dropped socket must SELF-HEAL — a fresh socket is
    // constructed and Subscribe re-sent with last_seen replay, status loud
    // (`reconnecting` → `live`) the whole way. A routine core reboot must never
    // permanently orphan an open renderer (glass-boxed 2026-07-29 on web).
    test('reconnects and resubscribes with last_seen after a drop', () async {
      final sockets = <FakeSocket>[];
      final statuses = <StateFeedStatus>[];
      final conn = StateConnection(
        'ws://x',
        socketFactory: (_) async {
          final s = FakeSocket();
          sockets.add(s);
          return s;
        },
      );
      conn.onStatus((s, _) => statuses.add(s));
      conn.on('chat', (_) {});
      await conn.connect();
      sockets.first.deliver(stateFrame('chat', 1, {}));
      await Future<void>.delayed(Duration.zero);
      expect(statuses, contains(StateFeedStatus.live));

      await sockets.first.drop(); // simulate core reboot
      await Future<void>.delayed(Duration.zero);
      expect(statuses.last, StateFeedStatus.reconnecting);

      // Ladder step 1 = 1s; give it room, then verify the fresh socket + replay.
      await Future<void>.delayed(const Duration(milliseconds: 1200));
      expect(sockets, hasLength(2));
      final sub = jsonDecode(sockets[1].sent.first) as Map<String, dynamic>;
      expect(sub['kinds'], contains('chat'));
      expect(sub['last_seen'], [
        {'kind': 'chat', 'revision': 1}
      ]);
      sockets[1].deliver(stateFrame('chat', 2, {}));
      await Future<void>.delayed(Duration.zero);
      expect(statuses.last, StateFeedStatus.live);
      await conn.close();
    });

    // what this catches: reconnect:false preserves the one-shot fail-loud
    // contract — a probe awaiting a dead core must SEE the failure, and the
    // default (reconnect on) must instead resolve + surface `reconnecting`.
    test('connect failure: fail-loud one-shot vs self-heal default', () async {
      final oneShot = StateConnection(
        'ws://x',
        socketFactory: (_) async => throw const SocketException('refused'),
        reconnect: false,
      );
      oneShot.on('chat', (_) {});
      await expectLater(oneShot.connect(), throwsA(isA<SocketException>()));

      final statuses = <StateFeedStatus>[];
      final healing = StateConnection(
        'ws://x',
        socketFactory: (_) async => throw const SocketException('refused'),
      );
      healing.onStatus((s, _) => statuses.add(s));
      healing.on('chat', (_) {});
      await healing.connect(); // resolves — last-known UI stays up, ladder runs
      expect(statuses, contains(StateFeedStatus.reconnecting));
      await healing.close();
      expect(statuses.last, StateFeedStatus.closed);
    });

    // what this catches: FileStateStorage (the on-device durable adapter, both
    // OSes) must behave identically to the Memory conformance reference —
    // replace-by-kind, survive process restarts, tolerate a corrupt file.
    test('FileStateStorage conforms: round-trip, replace-by-kind, corrupt-tolerant',
        () async {
      final dir = await Directory.systemTemp.createTemp('positron-state-test');
      try {
        final fs = FileStateStorage(dir);
        await fs.save('s', const StateEnvelope(kind: 'chat', revision: 1, layer: 'ephemeral', payload: 'v1'));
        await fs.save('s', const StateEnvelope(kind: 'nav', revision: 1, layer: 'ephemeral', payload: 'n1'));
        await fs.save('s', const StateEnvelope(kind: 'chat', revision: 2, layer: 'ephemeral', payload: 'v2'));
        // A second instance = a fresh process: must load what the first wrote.
        final rows = await FileStateStorage(dir).load('s');
        expect(rows, hasLength(2)); // chat replaced, nav kept
        final chat = rows.firstWhere((r) => r.envelope.kind == 'chat');
        expect(chat.envelope.payload, 'v2');
        // Corrupt the file — load degrades to empty (live-only), never throws.
        final f = dir.listSync().whereType<File>().first;
        await f.writeAsString('not json{');
        expect(await FileStateStorage(dir).load('s'), isEmpty);
      } finally {
        await dir.delete(recursive: true);
      }
    });
  });
}
