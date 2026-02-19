package me.go_gradually.omypic.presentation.datatransfer.controller;

import me.go_gradually.omypic.application.datatransfer.model.DataTransferImportResult;
import me.go_gradually.omypic.application.datatransfer.usecase.DataTransferUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.mock.web.MockMultipartFile;
import org.springframework.test.web.servlet.MockMvc;

import java.time.Instant;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.hamcrest.Matchers.containsString;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.multipart;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.header;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, DataTransferController.class})
@AutoConfigureMockMvc
class DataTransferControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private DataTransferUseCase dataTransferUseCase;

    @Test
    void exportData_returnsZipAttachment() throws Exception {
        when(dataTransferUseCase.exportZip()).thenReturn(new byte[]{1, 2, 3});

        mockMvc.perform(get("/api/data-transfer/export"))
                .andExpect(status().isOk())
                .andExpect(header().string(HttpHeaders.CONTENT_DISPOSITION, containsString("omypic-backup-")))
                .andExpect(header().string(HttpHeaders.CONTENT_DISPOSITION, containsString(".zip")))
                .andExpect(header().string(HttpHeaders.CONTENT_TYPE, "application/zip"));
    }

    @Test
    void importData_returnsImportSummary() throws Exception {
        when(dataTransferUseCase.importZip(any(byte[].class))).thenReturn(new DataTransferImportResult(
                Instant.parse("2026-02-19T00:00:00Z"),
                3,
                2,
                4,
                7,
                true
        ));

        mockMvc.perform(multipart("/api/data-transfer/import")
                        .file(new MockMultipartFile("file", "backup.zip", MediaType.APPLICATION_OCTET_STREAM_VALUE, new byte[]{9, 8, 7})))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.questionGroupCount").value(3))
                .andExpect(jsonPath("$.rulebookCount").value(2))
                .andExpect(jsonPath("$.wrongNoteCount").value(4))
                .andExpect(jsonPath("$.wrongNoteQueueSize").value(7))
                .andExpect(jsonPath("$.restartRequired").value(true));
        var captor = org.mockito.ArgumentCaptor.forClass(byte[].class);
        verify(dataTransferUseCase).importZip(captor.capture());
        assertArrayEquals(new byte[]{9, 8, 7}, captor.getValue());
    }
}
