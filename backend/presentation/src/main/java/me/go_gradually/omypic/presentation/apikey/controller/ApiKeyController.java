package me.go_gradually.omypic.presentation.apikey.controller;

import jakarta.validation.Valid;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyCommand;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.usecase.ApiKeyVerifyUseCase;
import me.go_gradually.omypic.presentation.apikey.dto.ApiKeyVerifyRequest;
import me.go_gradually.omypic.presentation.apikey.dto.ApiKeyVerifyResponse;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/keys")
public class ApiKeyController {
    private final ApiKeyVerifyUseCase apiKeyVerifyUseCase;

    public ApiKeyController(ApiKeyVerifyUseCase apiKeyVerifyUseCase) {
        this.apiKeyVerifyUseCase = apiKeyVerifyUseCase;
    }

    @PostMapping("/verify")
    public ApiKeyVerifyResponse verify(@Valid @RequestBody ApiKeyVerifyRequest request) {
        ApiKeyVerifyCommand command = new ApiKeyVerifyCommand();
        command.setProvider(request.getProvider());
        command.setApiKey(request.getApiKey());
        command.setModel(request.getModel());

        ApiKeyVerifyResult result = apiKeyVerifyUseCase.verify(command);
        ApiKeyVerifyResponse response = new ApiKeyVerifyResponse();
        response.setValid(result.isValid());
        response.setProvider(result.getProvider());
        response.setCheckedAt(result.getCheckedAt());
        response.setMessage(result.getMessage());
        return response;
    }
}
