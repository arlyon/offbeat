// OFFBEAT StarButton — Star toggle (★/☆)
// Accent color when on, fg4 when off
// Scale animation on press

import 'package:flutter/material.dart';
import '../theme/tokens.dart';

class StarButton extends StatefulWidget {
  final bool starred;
  final VoidCallback onToggle;
  final double size;

  const StarButton({
    super.key,
    required this.starred,
    required this.onToggle,
    this.size = 18,
  });

  @override
  State<StarButton> createState() => _StarButtonState();
}

class _StarButtonState extends State<StarButton>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _scale;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 80),
    );
    _scale = Tween<double>(
      begin: 1.0,
      end: 0.88,
    ).animate(CurvedAnimation(parent: _controller, curve: Curves.easeOut));
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _handleTap() {
    _controller.forward().then((_) {
      _controller.reverse();
    });
    widget.onToggle();
  }

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: _handleTap,
      child: AnimatedBuilder(
        animation: _scale,
        builder: (context, child) => Transform.scale(
          scale: _scale.value,
          child: Text(
            widget.starred ? '★' : '☆',
            style: TextStyle(
              fontSize: widget.size,
              color: widget.starred ? colorAccent : colorFg4,
              height: 1,
            ),
          ),
        ),
      ),
    );
  }
}
