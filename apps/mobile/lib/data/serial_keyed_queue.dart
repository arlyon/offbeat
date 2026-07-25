/// Serialises asynchronous mutations per logical key while allowing unrelated
/// keys to proceed independently.
class SerialKeyedQueue {
  final Map<String, Future<void>> _tails = {};

  Future<void> enqueue(String key, Future<void> Function() action) {
    final previous = _tails[key] ?? Future<void>.value();
    final execution = previous.then((_) => action());
    final tail = execution.then<void>((_) {}, onError: (_, _) {});
    _tails[key] = tail;
    tail.whenComplete(() {
      if (identical(_tails[key], tail)) _tails.remove(key);
    });
    return execution;
  }
}
