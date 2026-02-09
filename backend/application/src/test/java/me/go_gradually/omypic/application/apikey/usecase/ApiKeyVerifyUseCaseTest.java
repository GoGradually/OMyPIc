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
        ApiKeyVerifyCommand command = command("gemini", "AIza-test-key", null);

        ApiKeyVerifyResult result = useCase.verify(command);

        assertTrue(result.isValid());
        verify(probePort).probe("gemini", "AIza-test-key", null);
    }

    @Test
    void verify_returnsFailure_whenProbeFails() throws Exception {
        ApiKeyVerifyCommand command = command("anthropic", "sk-ant-test", null);
        doThrow(new IllegalStateException("401")).when(probePort).probe("anthropic", "sk-ant-test", null);

        ApiKeyVerifyResult result = useCase.verify(command);

        assertFalse(result.isValid());
        assertEquals("401", result.getMessage());
    }

    private ApiKeyVerifyCommand command(String provider, String key, String model) {
        ApiKeyVerifyCommand command = new ApiKeyVerifyCommand();
        command.setProvider(provider);
        command.setApiKey(key);
        command.setModel(model);
        return command;
    }
}
