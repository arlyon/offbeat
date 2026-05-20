import 'package:flutter/material.dart';

void main() {
  runApp(const OffbeatApp());
}

class OffbeatApp extends StatelessWidget {
  const OffbeatApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
      title: 'OFFBEAT',
      home: Scaffold(
        body: Center(
          child: Text('OFFBEAT'),
        ),
      ),
    );
  }
}
