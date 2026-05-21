// OFFBEAT LiveDot — Pulsing magenta circle
// 7px diameter, accent color, 50% border-radius (the ONLY round element)
// Pulsing box-shadow animation (1.6s)

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class LiveDot extends StatefulWidget {
  final double size;
  const LiveDot({super.key, this.size = 7});

  @override
  State<LiveDot> createState() => _LiveDotState();
}

class _LiveDotState extends State<LiveDot> with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _pulse;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1600),
    )..repeat();
    _pulse = Tween<double>(begin: 0.0, end: 1.0).animate(
      CurvedAnimation(
        parent: _controller,
        curve: const Interval(0.0, 0.7, curve: Curves.easeOut),
      ),
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: widget.size,
      height: widget.size,
      child: AnimatedBuilder(
        animation: _controller,
        builder: (context, child) {
          final t = _pulse.value;
          final ringSize = widget.size + t * (widget.size * 2.8);
          final ringOpacity = (1.0 - t) * 0.55;
          return Stack(
            clipBehavior: Clip.none,
            alignment: Alignment.center,
            children: [
              // Pulsing ring (overflows visually, doesn't affect layout)
              Positioned(
                left: (widget.size - ringSize) / 2,
                top: (widget.size - ringSize) / 2,
                child: Container(
                  width: ringSize,
                  height: ringSize,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: colorAccent.withOpacity(ringOpacity),
                  ),
                ),
              ),
              // Core dot
              child!,
            ],
          );
        },
        child: Container(
          width: widget.size,
          height: widget.size,
          decoration: const BoxDecoration(
            shape: BoxShape.circle,
            color: colorAccent,
          ),
        ),
      ),
    );
  }
}
