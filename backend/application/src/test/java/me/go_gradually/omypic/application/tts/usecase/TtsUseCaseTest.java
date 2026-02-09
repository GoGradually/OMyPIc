package me.go_gradually.omypic.application.tts.usecase;

import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.port.TtsGateway;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InOrder;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.io.IOException;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class TtsUseCaseTest {

    @Mock
    private TtsGateway ttsGateway;
    @Mock
    private MetricsPort metrics;
    @Mock
    private AudioSink sink;

    private TtsUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new TtsUseCase(ttsGateway, metrics);
    }

    @Test
    void stream_writesAllChunksInOrder_andRecordsLatency() throws Exception {
        TtsCommand command = new TtsCommand("hello", "alloy");
        byte[] first = new byte[]{1, 2};
        byte[] second = new byte[]{3, 4};

        when(ttsGateway.stream("key", command)).thenReturn(List.of(first, second));

        useCase.stream("key", command, sink);

        InOrder inOrder = inOrder(sink, metrics);
        inOrder.verify(sink).write(first);
        inOrder.verify(sink).write(second);
        inOrder.verify(metrics).recordTtsLatency(any());
        verify(metrics, never()).incrementTtsError();
    }

    @Test
    void stream_recordsErrorAndRethrows_whenSinkWriteFails() throws Exception {
        TtsCommand command = new TtsCommand("hello", "alloy");
        byte[] first = new byte[]{1, 2};
        when(ttsGateway.stream("key", command)).thenReturn(List.of(first));
        IOException ioException = new IOException("sink failed");
        org.mockito.Mockito.doThrow(ioException).when(sink).write(first);

        assertThrows(IOException.class, () -> useCase.stream("key", command, sink));

        verify(metrics).recordTtsLatency(any());
        verify(metrics).incrementTtsError();
    }

    @Test
    void stream_recordsErrorAndRethrows_whenGatewayFails() throws Exception {
        TtsCommand command = new TtsCommand("hello", "alloy");
        RuntimeException gatewayException = new RuntimeException("gateway failed");
        when(ttsGateway.stream("key", command)).thenThrow(gatewayException);

        assertThrows(RuntimeException.class, () -> useCase.stream("key", command, sink));

        verify(metrics).recordTtsLatency(any());
        verify(metrics).incrementTtsError();
    }
}
