// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.swim;

import com.solacesystems.jms.SolConnectionFactory;
import com.solacesystems.jms.SolJmsUtility;
import com.fasterxml.jackson.annotation.JsonInclude;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;

import javax.jms.BytesMessage;
import javax.jms.Connection;
import javax.jms.ConnectionFactory;
import javax.jms.DeliveryMode;
import javax.jms.Destination;
import javax.jms.JMSException;
import javax.jms.MapMessage;
import javax.jms.Message;
import javax.jms.MessageConsumer;
import javax.jms.ObjectMessage;
import javax.jms.Queue;
import javax.jms.Session;
import javax.jms.StreamMessage;
import javax.jms.TextMessage;
import java.io.Closeable;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.SQLException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Enumeration;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

public final class SwimNotamsConsumer {
    private static final ObjectMapper JSON_PRETTY = new ObjectMapper()
            .setSerializationInclusion(JsonInclude.Include.NON_NULL)
            .enable(SerializationFeature.INDENT_OUTPUT);
    private static final ObjectMapper JSON_LINE = new ObjectMapper()
            .setSerializationInclusion(JsonInclude.Include.NON_NULL);

    public static void main(String[] args) throws Exception {
        CliArgs cli = CliArgs.parse(args);
        SwimConfig config = JSON_PRETTY.readValue(cli.configPath.toFile(), SwimConfig.class);
        config.validate();

        if (cli.maxMessagesOverride != null) {
            config.maxMessages = cli.maxMessagesOverride;
        }
        if (cli.receiveTimeoutMillisOverride != null) {
            config.receiveTimeoutMillis = cli.receiveTimeoutMillisOverride;
        }

        Summary summary = new Summary();
        summary.startedAtUtc = Instant.now().toString();
        summary.queue = config.queue;
        summary.providerUrl = config.providerUrl;
        summary.normalizedProviderUrl = normalizeProviderUrl(config.providerUrl);
        summary.connectionFactory = config.connectionFactory;
        summary.vpn = config.vpn;
        summary.maxMessages = config.maxMessages;
        summary.receiveTimeoutMillis = config.receiveTimeoutMillis;
        System.err.printf(
                "SWIM NOTAM collector connecting providerUrl=%s normalizedProviderUrl=%s vpn=%s queue=%s%n",
                summary.providerUrl,
                summary.normalizedProviderUrl,
                summary.vpn,
                summary.queue
        );

        Connection connection = null;
        Session session = null;
        MessageConsumer consumer = null;
        try (RawMessageStore store = RawMessageStore.open(cli.sqlitePath)) {
            ConnectionFactory factory = createConnectionFactory(config);
            connection = factory.createConnection();
            session = connection.createSession(false, Session.CLIENT_ACKNOWLEDGE);
            Queue queue = SolJmsUtility.createQueue(config.queue);
            consumer = session.createConsumer(queue);
            connection.start();

            while (config.maxMessages == 0 || summary.messageCount < config.maxMessages) {
                Message message = consumer.receive(config.receiveTimeoutMillis);
                if (message == null) {
                    continue;
                }

                CapturedMessage captured = capture(message, summary.messageCount + 1);
                store.insert(captured);

                message.acknowledge();

                summary.messageCount += 1;
                summary.messageTypes.merge(captured.messageClass, 1, Integer::sum);
                if (summary.firstReceivedAtUtc == null) {
                    summary.firstReceivedAtUtc = captured.receivedAtUtc;
                }
                summary.lastReceivedAtUtc = captured.receivedAtUtc;
                summary.totalBodyBytes += captured.bodySizeBytes;
            }
            if (summary.exitReason == null) {
                summary.exitReason = config.maxMessages > 0 && summary.messageCount >= config.maxMessages
                        ? "max_messages"
                        : "completed";
            }
        } finally {
            closeQuietly(consumer);
            closeQuietly(session);
            closeQuietly(connection);
        }

        summary.finishedAtUtc = Instant.now().toString();
        System.out.println(JSON_PRETTY.writeValueAsString(summary));
    }

    private static ConnectionFactory createConnectionFactory(SwimConfig config) throws Exception {
        java.util.Hashtable<String, Object> env = new java.util.Hashtable<>();
        if (config.trustStorePath != null && !config.trustStorePath.isBlank()) {
            env.put("Solace_JMS_SSL_TrustStore", config.trustStorePath);
        }
        if (config.trustStoreFormat != null && !config.trustStoreFormat.isBlank()) {
            env.put("Solace_JMS_SSL_TrustStoreFormat", config.trustStoreFormat);
        }
        if (config.trustStorePassword != null && !config.trustStorePassword.isBlank()) {
            env.put("Solace_JMS_SSL_TrustStorePassword", config.trustStorePassword);
        }
        SolConnectionFactory factory = SolJmsUtility.createConnectionFactory(
                normalizeProviderUrl(config.providerUrl),
                config.username,
                config.password,
                config.vpn,
                env
        );
        return factory;
    }

    private static String normalizeProviderUrl(String providerUrl) {
        if (providerUrl.startsWith("tcps://")) {
            return "smfs://" + providerUrl.substring("tcps://".length());
        }
        if (providerUrl.startsWith("tcp://")) {
            return "smf://" + providerUrl.substring("tcp://".length());
        }
        return providerUrl;
    }

    private static CapturedMessage capture(Message message, int sequence) throws JMSException {
        CapturedMessage out = new CapturedMessage();
        out.sequence = sequence;
        out.receivedAtUtc = Instant.now().toString();
        out.messageClass = message.getClass().getSimpleName();
        out.jmsMessageId = message.getJMSMessageID();
        out.jmsCorrelationId = message.getJMSCorrelationID();
        out.jmsType = message.getJMSType();
        out.jmsTimestamp = message.getJMSTimestamp();
        out.jmsExpiration = message.getJMSExpiration();
        out.jmsPriority = message.getJMSPriority();
        out.jmsRedelivered = message.getJMSRedelivered();
        out.jmsDeliveryMode = deliveryModeName(message.getJMSDeliveryMode());
        out.destination = destinationName(message.getJMSDestination());
        out.replyTo = destinationName(message.getJMSReplyTo());
        out.properties = readProperties(message);

        if (message instanceof TextMessage) {
            TextMessage textMessage = (TextMessage) message;
            out.bodyText = textMessage.getText();
            out.bodySizeBytes = out.bodyText == null ? 0 : out.bodyText.getBytes(StandardCharsets.UTF_8).length;
        } else if (message instanceof BytesMessage) {
            BytesMessage bytesMessage = (BytesMessage) message;
            byte[] body = readBytes(bytesMessage);
            out.bodyBase64 = Base64.getEncoder().encodeToString(body);
            out.bodyUtf8 = decodeUtf8(body);
            out.bodySizeBytes = body.length;
        } else if (message instanceof MapMessage) {
            MapMessage mapMessage = (MapMessage) message;
            out.bodyMap = readMap(mapMessage);
            out.bodySizeBytes = JSONSize.of(out.bodyMap);
        } else if (message instanceof ObjectMessage) {
            ObjectMessage objectMessage = (ObjectMessage) message;
            Object object = objectMessage.getObject();
            out.bodyObjectString = object == null ? null : object.toString();
            out.bodySizeBytes = out.bodyObjectString == null ? 0 : out.bodyObjectString.getBytes(StandardCharsets.UTF_8).length;
        } else if (message instanceof StreamMessage) {
            StreamMessage streamMessage = (StreamMessage) message;
            List<String> parts = readStream(streamMessage);
            out.bodyStream = parts;
            out.bodySizeBytes = JSONSize.of(parts);
        }
        return out;
    }

    private static Map<String, Object> readProperties(Message message) throws JMSException {
        Map<String, Object> properties = new TreeMap<>();
        Enumeration<?> names = message.getPropertyNames();
        while (names.hasMoreElements()) {
            String key = String.valueOf(names.nextElement());
            Object value = message.getObjectProperty(key);
            properties.put(key, value);
        }
        return properties;
    }

    private static byte[] readBytes(BytesMessage message) throws JMSException {
        long bodyLength = message.getBodyLength();
        if (bodyLength > Integer.MAX_VALUE) {
            throw new JMSException("message body too large: " + bodyLength);
        }
        byte[] body = new byte[(int) bodyLength];
        int offset = 0;
        while (offset < body.length) {
            int read = message.readBytes(body, body.length - offset);
            if (read <= 0) {
                break;
            }
            offset += read;
        }
        if (offset == body.length) {
            return body;
        }
        byte[] truncated = new byte[offset];
        System.arraycopy(body, 0, truncated, 0, offset);
        return truncated;
    }

    private static String decodeUtf8(byte[] body) {
        try {
            return new String(body, StandardCharsets.UTF_8);
        } catch (Exception ex) {
            return null;
        }
    }

    private static Map<String, Object> readMap(MapMessage message) throws JMSException {
        Map<String, Object> map = new TreeMap<>();
        Enumeration<?> names = message.getMapNames();
        while (names.hasMoreElements()) {
            String key = String.valueOf(names.nextElement());
            map.put(key, message.getObject(key));
        }
        return map;
    }

    private static List<String> readStream(StreamMessage message) throws JMSException {
        List<String> values = new ArrayList<>();
        while (true) {
            try {
                Object value = message.readObject();
                values.add(String.valueOf(value));
            } catch (javax.jms.MessageEOFException eof) {
                return values;
            }
        }
    }

    private static String destinationName(Destination destination) {
        if (destination == null) {
            return null;
        }
        try {
            if (destination instanceof Queue) {
                Queue queue = (Queue) destination;
                return queue.getQueueName();
            }
            return destination.toString();
        } catch (JMSException ex) {
            return destination.toString();
        }
    }

    private static String deliveryModeName(int mode) {
        if (mode == DeliveryMode.PERSISTENT) {
            return "PERSISTENT";
        }
        if (mode == DeliveryMode.NON_PERSISTENT) {
            return "NON_PERSISTENT";
        }
        return Integer.toString(mode);
    }

    private static final class RawMessageStore implements AutoCloseable {
        private final java.sql.Connection connection;
        private final PreparedStatement insert;

        private RawMessageStore(java.sql.Connection connection) throws SQLException {
            this.connection = connection;
            this.insert = connection.prepareStatement(
                    "INSERT OR IGNORE INTO raw_notam_messages ("
                            + "dedupe_key, jms_message_id, received_at_utc, message_json, message_sha256, committed_at_utc"
                            + ") VALUES (?, ?, ?, ?, ?, ?)"
            );
        }

        static RawMessageStore open(Path sqlitePath) throws Exception {
            Path parent = sqlitePath.getParent();
            if (parent != null) {
                Files.createDirectories(parent);
            }
            java.sql.Connection connection = DriverManager.getConnection("jdbc:sqlite:" + sqlitePath);
            try (java.sql.Statement statement = connection.createStatement()) {
                statement.execute("PRAGMA busy_timeout = 5000");
                statement.execute("PRAGMA journal_mode = WAL");
                statement.execute("PRAGMA synchronous = FULL");
                statement.execute(
                        "CREATE TABLE IF NOT EXISTS raw_notam_messages ("
                                + "ingest_seq INTEGER PRIMARY KEY AUTOINCREMENT,"
                                + "dedupe_key TEXT NOT NULL UNIQUE,"
                                + "jms_message_id TEXT,"
                                + "received_at_utc TEXT NOT NULL,"
                                + "message_json TEXT NOT NULL,"
                                + "message_sha256 TEXT NOT NULL,"
                                + "committed_at_utc TEXT NOT NULL,"
                                + "applied_at_utc TEXT"
                                + ")"
                );
                statement.execute(
                        "CREATE INDEX IF NOT EXISTS raw_notam_messages_applied_idx "
                                + "ON raw_notam_messages(applied_at_utc, ingest_seq)"
                );
            }
            connection.setAutoCommit(false);
            return new RawMessageStore(connection);
        }

        void insert(CapturedMessage captured) throws Exception {
            String messageJson = JSON_LINE.writeValueAsString(captured);
            String messageSha256 = sha256Hex(messageJson);
            insert.setString(1, dedupeKey(captured, messageSha256));
            insert.setString(2, captured.jmsMessageId);
            insert.setString(3, captured.receivedAtUtc);
            insert.setString(4, messageJson);
            insert.setString(5, messageSha256);
            insert.setString(6, Instant.now().toString());
            insert.executeUpdate();
            connection.commit();
        }

        @Override
        public void close() throws SQLException {
            insert.close();
            connection.close();
        }
    }

    private static String dedupeKey(CapturedMessage captured, String messageSha256) {
        if (captured.jmsMessageId != null && !captured.jmsMessageId.isBlank()) {
            return "jms:" + captured.jmsMessageId;
        }
        return "sha256:" + messageSha256;
    }

    private static String sha256Hex(String value) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        byte[] hash = digest.digest(value.getBytes(StandardCharsets.UTF_8));
        StringBuilder out = new StringBuilder(hash.length * 2);
        for (byte b : hash) {
            out.append(String.format("%02x", b));
        }
        return out.toString();
    }

    private static void closeQuietly(Object closeable) {
        if (closeable == null) {
            return;
        }
        try {
            if (closeable instanceof MessageConsumer) {
                MessageConsumer consumer = (MessageConsumer) closeable;
                consumer.close();
            } else if (closeable instanceof Session) {
                Session session = (Session) closeable;
                session.close();
            } else if (closeable instanceof Connection) {
                Connection connection = (Connection) closeable;
                connection.close();
            } else if (closeable instanceof Closeable) {
                Closeable io = (Closeable) closeable;
                io.close();
            }
        } catch (Exception ignored) {
        }
    }

    static final class CliArgs {
        final Path configPath;
        final Path sqlitePath;
        final Integer maxMessagesOverride;
        final Long receiveTimeoutMillisOverride;

        CliArgs(
                Path configPath,
                Path sqlitePath,
                Integer maxMessagesOverride,
                Long receiveTimeoutMillisOverride
        ) {
            this.configPath = configPath;
            this.sqlitePath = sqlitePath;
            this.maxMessagesOverride = maxMessagesOverride;
            this.receiveTimeoutMillisOverride = receiveTimeoutMillisOverride;
        }

        static CliArgs parse(String[] args) {
            Path configPath = null;
            Path sqlitePath = null;
            Integer maxMessages = null;
            Long receiveTimeoutMillis = null;
            for (int i = 0; i < args.length; i += 2) {
                if (i + 1 >= args.length) {
                    throw new IllegalArgumentException(usage());
                }
                String key = args[i];
                String value = args[i + 1];
                switch (key) {
                    case "--config":
                        configPath = Path.of(value);
                        break;
                    case "--sqlite":
                        sqlitePath = Path.of(value);
                        break;
                    case "--max-messages":
                        maxMessages = Integer.parseInt(value);
                        break;
                    case "--receive-timeout-ms":
                        receiveTimeoutMillis = Long.parseLong(value);
                        break;
                    default:
                        throw new IllegalArgumentException(usage());
                }
            }
            if (configPath == null || sqlitePath == null) {
                throw new IllegalArgumentException(usage());
            }
            return new CliArgs(configPath, sqlitePath, maxMessages, receiveTimeoutMillis);
        }

        static String usage() {
            return "usage: java -jar swim-notams-fetch.jar --config <path> --sqlite <path> "
                    + "[--max-messages <count>] [--receive-timeout-ms <ms>]";
        }
    }

    public static final class SwimConfig {
        public String aerobagEnvironment;
        public String providerUrl;
        public String queue;
        public String connectionFactory;
        public String username;
        public String password;
        public String vpn;
        public Integer maxMessages = 0;
        public Long receiveTimeoutMillis = 2000L;
        public String trustStorePath;
        public String trustStoreFormat;
        public String trustStorePassword;

        void validate() {
            require(providerUrl, "providerUrl");
            require(queue, "queue");
            require(connectionFactory, "connectionFactory");
            require(username, "username");
            require(password, "password");
            require(vpn, "vpn");
            if (maxMessages == null || maxMessages < 0) {
                throw new IllegalArgumentException("maxMessages must be non-negative");
            }
            if (receiveTimeoutMillis == null || receiveTimeoutMillis <= 0) {
                throw new IllegalArgumentException("receiveTimeoutMillis must be positive");
            }
        }

        private static void require(String value, String name) {
            if (value == null || value.isBlank()) {
                throw new IllegalArgumentException("missing required config field: " + name);
            }
        }
    }

    public static final class CapturedMessage {
        public int sequence;
        public String receivedAtUtc;
        public String messageClass;
        public String jmsMessageId;
        public String jmsCorrelationId;
        public String jmsType;
        public long jmsTimestamp;
        public long jmsExpiration;
        public int jmsPriority;
        public boolean jmsRedelivered;
        public String jmsDeliveryMode;
        public String destination;
        public String replyTo;
        public Map<String, Object> properties;
        public String bodyText;
        public String bodyUtf8;
        public String bodyBase64;
        public Map<String, Object> bodyMap;
        public List<String> bodyStream;
        public String bodyObjectString;
        public int bodySizeBytes;
    }

    public static final class Summary {
        public String startedAtUtc;
        public String finishedAtUtc;
        public String providerUrl;
        public String normalizedProviderUrl;
        public String queue;
        public String connectionFactory;
        public String vpn;
        public int maxMessages;
        public long receiveTimeoutMillis;
        public int messageCount;
        public long totalBodyBytes;
        public String exitReason;
        public String firstReceivedAtUtc;
        public String lastReceivedAtUtc;
        public Map<String, Integer> messageTypes = new TreeMap<>();
    }

    private static final class JSONSize {
        static int of(Object value) {
            try {
                return JSON_LINE.writeValueAsBytes(value).length;
            } catch (IOException ex) {
                return 0;
            }
        }
    }
}
