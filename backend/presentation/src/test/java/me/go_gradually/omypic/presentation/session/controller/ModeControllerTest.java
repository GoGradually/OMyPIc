package me.go_gradually.omypic.presentation.session.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.session.model.InvalidGroupTagsException;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.presentation.TestBootApplication;
import me.go_gradually.omypic.presentation.shared.error.ApiExceptionHandler;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, ModeController.class, ApiExceptionHandler.class})
@AutoConfigureMockMvc
class ModeControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private SessionUseCase sessionUseCase;

    @Test
    void update_usesUpdateMode_withSelectedGroupTags() throws Exception {
        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s2",
                                "mode", "CONTINUOUS",
                                "continuousBatchSize", 5,
                                "selectedGroupTags", List.of("travel")
                        ))))
                .andExpect(status().isNoContent());

        ArgumentCaptor<ModeUpdateCommand> captor = ArgumentCaptor.forClass(ModeUpdateCommand.class);
        verify(sessionUseCase).updateMode(captor.capture());
        assertEquals("s2", captor.getValue().getSessionId());
        assertEquals(ModeType.CONTINUOUS, captor.getValue().getMode());
        assertEquals(5, captor.getValue().getContinuousBatchSize());
        assertEquals(List.of("travel"), captor.getValue().getSelectedGroupTags());
    }

    @Test
    void update_returnsBadRequest_whenModeIsUnsupported() throws Exception {
        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s3",
                                "mode", "MOCK_EXAM",
                                "selectedGroupTags", List.of("travel")
                        ))))
                .andExpect(status().isBadRequest());
    }

    @Test
    void update_returnsInvalidGroupTags_whenSelectedTagsAreEmpty() throws Exception {
        when(sessionUseCase.updateMode(any(ModeUpdateCommand.class)))
                .thenThrow(new InvalidGroupTagsException("selectedGroupTags must not be empty", List.of()));

        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s4",
                                "mode", "IMMEDIATE",
                                "selectedGroupTags", List.of()
                        ))))
                .andExpect(status().isBadRequest())
                .andExpect(jsonPath("$.code").value("INVALID_GROUP_TAGS"))
                .andExpect(jsonPath("$.invalidTags").isArray())
                .andExpect(jsonPath("$.invalidTags").isEmpty());
    }

    @Test
    void update_returnsInvalidGroupTags_whenSomeTagsAreUnknown() throws Exception {
        when(sessionUseCase.updateMode(any(ModeUpdateCommand.class)))
                .thenThrow(new InvalidGroupTagsException("Some selectedGroupTags are invalid", List.of("unknown")));

        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s5",
                                "mode", "IMMEDIATE",
                                "selectedGroupTags", List.of("travel", "unknown")
                        ))))
                .andExpect(status().isBadRequest())
                .andExpect(jsonPath("$.code").value("INVALID_GROUP_TAGS"))
                .andExpect(jsonPath("$.invalidTags[0]").value("unknown"));
    }
}
