package me.go_gradually.omypic.application.apikey.usecase;

import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyCommand;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class ApiKeyVerifyUseCaseTest {

    @Mock
    private ApiKeyProbePort probePort;

    private ApiKeyVerifyUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new ApiKeyVerifyUseCase(probePort);
    }

    @Test
    void verify_returnsFailure_whenFormatInvalid() {
        ApiKeyVerifyCommand command = command("openai", "invalid", null);

        ApiKeyVerifyResult result = useCase.verify(command);

        assertFalse(result.isValid());
        assertTrue(result.getMessage().contains("sk-"));
        verifyNoInteractions(probePort);
    }

    @Test
    void verify_returnsSuccess_whenProbePasses() throws Exception {
        ApiKeyVerifyCommand command = command("openai", "sk-test-key", null);

        ApiKeyVerifyResult result = useCase.verify(command);

        assertTrue(result.isValid());
        verify(probePort).probe("openai", "sk-test-key", null);
    }

    @Test
    void verify_returnsFailure_whenProbeFails() throws Exception {
        ApiKeyVerifyCommand command = command("openai", "sk-test", null);
        doThrow(new IllegalStateException("401")).when(probePort).probe("openai", "sk-test", null);

        ApiKeyVerifyResult result = useCase.verify(command);

        assertFalse(result.isValid());
        assertEquals("401", result.getMessage());
    }

    @Test
    void verify_throws_whenFeedbackModelIsUnsupported() {
        ApiKeyVerifyCommand command = command("openai", "sk-test-key", "gpt-5.2");

        IllegalArgumentException error = assertThrows(IllegalArgumentException.class, () -> useCase.verify(command));

        assertEquals("Unsupported feedback model: gpt-5.2", error.getMessage());
        verifyNoInteractions(probePort);
    }

    @Test
    void verify_throws_whenProviderIsUnsupported() {
        ApiKeyVerifyCommand command = command("gemini", "AIza-test-key", null);

        IllegalArgumentException error = assertThrows(IllegalArgumentException.class, () -> useCase.verify(command));

        assertEquals("Unsupported provider: only openai is allowed", error.getMessage());
        verifyNoInteractions(probePort);
    }

    private ApiKeyVerifyCommand command(String provider, String key, String model) {
        ApiKeyVerifyCommand command = new ApiKeyVerifyCommand();
        command.setProvider(provider);
        command.setApiKey(key);
        command.setModel(model);
        return command;
    }
}
