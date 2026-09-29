"""
Robot Framework library for MQTT testing via paho-mqtt v2.
Connects as an external subscriber to an emulated device's broker.
"""

import json
import time
import threading
from collections import defaultdict
from pathlib import Path

import paho.mqtt.client as mqtt


class MqttKeywords:
    """
    MQTT subscriber keywords for Robot Framework.
    ROBOT_LIBRARY_SCOPE = 'TEST SUITE' (persistent across tests).
    """

    ROBOT_LIBRARY_SCOPE = 'TEST SUITE'

    def __init__(self):
        self.client = None
        self.connected = threading.Event()
        self.message_queues = defaultdict(list)  # topic -> list of decoded payloads
        self.message_lock = threading.Lock()
        self.traffic_log_path = Path("mqtt_traffic.log")
        self.arrival_times = defaultdict(list)  # topic -> list of monotonic timestamps
        self.run_loop_thread = None

    def connect_external_subscriber(self, host, port=1883, client_id="tms-test-sub", timeout=60):
        """
        Connect to MQTT broker as an external subscriber.
        Starts network loop in background thread.
        Waits up to `timeout` seconds for CONNACK.

        Args:
            host: Broker hostname/IP
            port: Broker port (default 1883)
            client_id: MQTT client ID (default tms-test-sub)
            timeout: Max seconds to wait for connection (default 60)

        Raises:
            RuntimeError: If connection fails or times out
        """
        self.client = mqtt.Client(
            callback_api_version=mqtt.CallbackAPIVersion.VERSION2,
            client_id=client_id,
            clean_session=True
        )

        self.client.on_connect = self._on_connect
        self.client.on_message = self._on_message
        self.client.on_disconnect = self._on_disconnect

        try:
            self.client.connect(host, port, keepalive=60)
        except Exception as e:
            raise RuntimeError(f"Failed to connect to {host}:{port}: {e}")

        # Start network loop in background thread
        self.run_loop_thread = threading.Thread(target=self.client.loop_forever, daemon=True)
        self.run_loop_thread.start()

        # Wait for CONNACK
        if not self.connected.wait(timeout=timeout):
            raise RuntimeError(f"Connection timeout after {timeout}s")

    def _on_connect(self, client, userdata, connect_flags, reason_code, properties):
        """Callback on successful connection."""
        if reason_code.is_failure:
            raise RuntimeError(f"Connection failed: {reason_code}")
        self.connected.set()

    def _on_disconnect(self, client, userdata, disconnect_flags, reason_code, properties):
        """Callback on disconnect."""
        self.connected.clear()

    def _on_message(self, client, userdata, msg):
        """Callback on message arrival. Records topic, payload, and timestamp."""
        arrival_time = time.monotonic()
        payload_bytes = msg.payload

        # Try to decode JSON; store raw if it fails
        try:
            payload_dict = json.loads(payload_bytes.decode('utf-8'))
        except (json.JSONDecodeError, UnicodeDecodeError):
            payload_dict = {"__raw_bytes__": payload_bytes.hex(), "__decode_error__": True}

        with self.message_lock:
            self.message_queues[msg.topic].append(payload_dict)
            self.arrival_times[msg.topic].append(arrival_time)

        # Append to traffic log
        self._log_traffic(msg.topic, payload_bytes, arrival_time)

    def _log_traffic(self, topic, payload_bytes, arrival_time):
        """Append message to traffic log file."""
        try:
            with open(self.traffic_log_path, 'a') as f:
                f.write(f"{arrival_time:.6f}\t{topic}\t{payload_bytes.decode('utf-8', errors='replace')}\n")
        except Exception:
            pass  # Silently ignore logging errors

    def set_traffic_log(self, path):
        """Set the traffic log file path (default mqtt_traffic.log)."""
        self.traffic_log_path = Path(path)

    def subscribe(self, topic_filter, qos=0):
        """
        Subscribe to a topic filter.

        Args:
            topic_filter: Topic filter (e.g., 'tms/#', 'monitor/#')
            qos: QoS level (default 0)
        """
        if not self.client:
            raise RuntimeError("Not connected. Call connect_external_subscriber first.")
        self.client.subscribe(topic_filter, qos=qos)

    def wait_for_message_on_topic(self, topic, timeout=30):
        """
        Wait for a message on a specific topic.
        Returns the first message received (since connect or since clear_messages).

        Args:
            topic: Exact topic string to match
            timeout: Max seconds to wait (default 30)

        Returns:
            Decoded JSON payload (dict)

        Raises:
            RuntimeError: If timeout occurs
        """
        start_time = time.time()
        while True:
            with self.message_lock:
                if self.message_queues[topic]:
                    return self.message_queues[topic][0]

            elapsed = time.time() - start_time
            if elapsed > timeout:
                raise RuntimeError(f"Timeout waiting for message on topic '{topic}' after {timeout}s")

            time.sleep(0.1)

    def messages_on_topic(self, topic):
        """
        Get all messages received on a topic (since connect or clear_messages).

        Args:
            topic: Topic string

        Returns:
            List of decoded JSON payloads (dicts)
        """
        with self.message_lock:
            return list(self.message_queues[topic])

    def raw_text_messages_on_topic(self, topic):
        """
        Get all messages received on a topic as plain UTF-8 text lines.

        Monitor log lines (`monitor/<device_id>/console`) are plain text
        (`LEVEL:target: message`), not JSON, so `messages_on_topic` stores them
        as `{"__raw_bytes__": <hex>, "__decode_error__": True}` fallback dicts.
        This keyword decodes those hex payloads back to text for substring/glob
        matching. Payloads that happened to parse as JSON are re-serialized.

        Args:
            topic: Topic string

        Returns:
            List of decoded text strings, in arrival order
        """
        with self.message_lock:
            payloads = list(self.message_queues[topic])

        lines = []
        for payload in payloads:
            if isinstance(payload, dict) and payload.get('__decode_error__'):
                lines.append(bytes.fromhex(payload['__raw_bytes__']).decode('utf-8', errors='replace'))
            elif isinstance(payload, str):
                lines.append(payload)
            else:
                lines.append(json.dumps(payload))
        return lines

    def latest_payload_on_topic(self, topic):
        """
        Get the most recent message on a topic.

        Args:
            topic: Topic string

        Returns:
            Decoded JSON payload (dict) or None if no messages
        """
        with self.message_lock:
            queue = self.message_queues[topic]
            return queue[-1] if queue else None

    def no_message_on_topic_within(self, topic, seconds):
        """
        Assert that NO message arrives on a topic within a time window.

        Args:
            topic: Topic string
            seconds: Time window (seconds)

        Raises:
            RuntimeError: If a message arrives within the window
        """
        with self.message_lock:
            initial_count = len(self.message_queues[topic])

        time.sleep(seconds)

        with self.message_lock:
            final_count = len(self.message_queues[topic])

        if final_count > initial_count:
            raise RuntimeError(
                f"Expected no message on topic '{topic}' within {seconds}s, "
                f"but {final_count - initial_count} message(s) arrived"
            )

    def get_json_field(self, payload, dotted_path):
        """
        Navigate a dict/list by dotted path.
        Examples: 'external_measurement.0.line.0.amperage'

        Args:
            payload: Dictionary or list to navigate
            dotted_path: Dot-separated path with optional array indices

        Returns:
            The value at the path

        Raises:
            KeyError, IndexError, or TypeError if path is invalid
        """
        parts = dotted_path.split('.')
        current = payload

        for part in parts:
            # Try to interpret as array index
            if isinstance(current, list):
                try:
                    index = int(part)
                    current = current[index]
                except (ValueError, IndexError) as e:
                    raise RuntimeError(f"Invalid array index '{part}': {e}")
            elif isinstance(current, dict):
                if part not in current:
                    raise KeyError(f"Key '{part}' not found in {list(current.keys())}")
                current = current[part]
            else:
                raise TypeError(f"Cannot navigate into {type(current).__name__} with key '{part}'")

        return current

    def clear_messages(self):
        """Clear all message queues and arrival times."""
        with self.message_lock:
            self.message_queues.clear()
            self.arrival_times.clear()

    def arrival_time_of_nth(self, topic, n):
        """
        Get the host-monotonic arrival time of the n-th message on a topic.

        Args:
            topic: Topic string
            n: Message index (0-based)

        Returns:
            Monotonic timestamp (float seconds)

        Raises:
            IndexError: If n is out of range
        """
        with self.message_lock:
            times = self.arrival_times[topic]
            if n >= len(times):
                raise IndexError(f"Only {len(times)} message(s) on topic '{topic}', requested index {n}")
            return times[n]

    def write_perf_metrics(self, path, metrics_dict):
        """
        Write performance metrics to a JSON file.

        Args:
            path: File path
            metrics_dict: Dictionary of metrics
        """
        with open(path, 'w') as f:
            json.dump(metrics_dict, f, indent=2)

    def disconnect_subscriber(self):
        """Disconnect the MQTT client."""
        if self.client:
            self.client.disconnect()
            self.client.loop_stop()
            if self.run_loop_thread:
                self.run_loop_thread.join(timeout=5)
