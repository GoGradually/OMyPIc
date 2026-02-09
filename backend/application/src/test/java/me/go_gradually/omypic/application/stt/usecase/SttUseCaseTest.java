package me.go_gradually.omypic.application.stt.usecase;

import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import me.go_gradually.omypic.application.stt.port.SttGateway;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class SttUseCaseTest {

    @Mock
    private SttGateway sttGateway;
    @Mock
    private SttPolicy sttPolicy;
    @Mock
    private MetricsPort metrics;

    private SttUseCase useCase;
    private VadSettings vadSettings;

    private static SttCommand commandWithBytes(int size) {
        SttCommand command = new SttCommand();
        command.setFileBytes(new byte[size]);
        command.setModel("gpt-4o-mini-transcribe");
        command.setApiKey("key");
        command.setTranslate(false);
        return command;
    }

    @BeforeEach
    void setUp() {
        vadSettings = new VadSettings(300, 500, 0.6);
        useCase = new SttUseCase(sttGateway, sttPolicy, metrics);
    }

    @Test
    void transcribe_retriesAndSucceedsWithinRetryLimit() throws Exception {
        stubPolicy(100L, 2);
        when(sttGateway.transcribe(any(), anyString(), anyString(), anyBoolean(), eq(vadSettings)))
                .thenThrow(new RuntimeException("temporary-1"))
                .thenThrow(new RuntimeException("temporary-2"))
                .thenReturn("final-text");

        String result = useCase.transcribe(commandWithBytes(10));

        assertEquals("final-text", result);
        verify(sttGateway, times(3)).transcribe(any(), eq("gpt-4o-mini-transcribe"), eq("key"), eq(false), eq(vadSettings));
        verify(metrics).incrementSttRequest();
        verify(metrics).recordSttLatency(any());
        verify(metrics, never()).incrementSttError();
    }

    @Test
    void transcribe_throwsAndIncrementsError_whenRetriesExhausted() throws Exception {
        stubPolicy(100L, 2);
        when(sttGateway.transcribe(any(), anyString(), anyString(), anyBoolean(), eq(vadSettings)))
                .thenThrow(new RuntimeException("always-fail"));

        assertThrows(IllegalStateException.class, () -> useCase.transcribe(commandWithBytes(10)));

        verify(sttGateway, times(3)).transcribe(any(), anyString(), anyString(), anyBoolean(), eq(vadSettings));
        verify(metrics).incrementSttRequest();
        verify(metrics, never()).recordSttLatency(any());
        verify(metrics).incrementSttError();
    }

    @Test
    void transcribe_rejectsNullOrTooLargeFiles() throws Exception {
        when(sttPolicy.getMaxFileBytes()).thenReturn(100L);
        SttCommand nullFile = commandWithBytes(1);
        nullFile.setFileBytes(null);

        SttCommand tooLarge = commandWithBytes(101);

        assertThrows(IllegalArgumentException.class, () -> useCase.transcribe(nullFile));
        assertThrows(IllegalArgumentException.class, () -> useCase.transcribe(tooLarge));

        verify(sttGateway, never()).transcribe(any(), anyString(), anyString(), anyBoolean(), any());
        verify(metrics, never()).incrementSttRequest();
    }

    @Test
    void transcribe_passesModelApiKeyTranslateAndVadToGateway() throws Exception {
        stubPolicy(100L, 2);
        SttCommand command = commandWithBytes(5);
        command.setModel("whisper-1");
        command.setApiKey("secret");
        command.setTranslate(true);
        when(sttGateway.transcribe(command.getFileBytes(), "whisper-1", "secret", true, vadSettings)).thenReturn("ok");

        String result = useCase.transcribe(command);

        assertEquals("ok", result);
        verify(metrics).incrementSttRequest();
        verify(sttGateway).transcribe(command.getFileBytes(), "whisper-1", "secret", true, vadSettings);
    }

    @Test
    void transcribe_withRetryMaxZero_attemptsOnlyOnce() throws Exception {
        stubPolicy(100L, 0);
        when(sttGateway.transcribe(any(), anyString(), anyString(), anyBoolean(), eq(vadSettings)))
                .thenThrow(new RuntimeException("fail-fast"));

        assertThrows(IllegalStateException.class, () -> useCase.transcribe(commandWithBytes(5)));

        verify(sttGateway, times(1)).transcribe(any(), anyString(), anyString(), anyBoolean(), eq(vadSettings));
        verify(metrics).incrementSttRequest();
        verify(metrics).incrementSttError();
    }

    private void stubPolicy(long maxFileBytes, int retryMax) {
        when(sttPolicy.getMaxFileBytes()).thenReturn(maxFileBytes);
        when(sttPolicy.retryMax()).thenReturn(retryMax);
        when(sttPolicy.getVadSettings()).thenReturn(vadSettings);
    }
}
