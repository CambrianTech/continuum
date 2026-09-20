// Widget smoke test — the Flutter painter renders the mobile screen.
//
// Proves the painter mounts and shows the mobile adaptation: the room in the app bar,
// the conversation (primary), and the Chat/Who/Where bottom nav. The adaptation RULE is
// tested in TS (mobileScreen.spec.ts); this proves the Dart painter consumes it.

import 'package:flutter_test/flutter_test.dart';
import 'package:continuum_mobile/main.dart';

void main() {
  testWidgets('renders room, conversation, and who/where bottom nav', (tester) async {
    await tester.pumpWidget(const ContinuumMobileApp());

    expect(find.text('cambriantech'), findsOneWidget); // app bar = room
    expect(find.text('Solenne'), findsWidgets); // a sender in the conversation (primary)
    expect(find.text('Chat'), findsOneWidget); // bottom nav destinations
    expect(find.text('Who'), findsOneWidget);
    expect(find.text('Where'), findsOneWidget);

    // Tapping "Who" swaps the body to the roster tab (secondary behind nav).
    await tester.tap(find.text('Who'));
    await tester.pumpAndSettle();
    expect(find.text('Asha'), findsWidgets); // roster member, dossier-free
  });
}
