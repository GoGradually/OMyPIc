package me.go_gradually.omypic.presentation.datatransfer.controller;

import me.go_gradually.omypic.application.datatransfer.model.DataTransferImportResult;
import me.go_gradually.omypic.application.datatransfer.usecase.DataTransferUseCase;
import me.go_gradually.omypic.presentation.datatransfer.dto.DataTransferImportResponse;
import org.springframework.http.ContentDisposition;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.multipart.MultipartFile;

import java.io.IOException;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;

@RestController
@RequestMapping("/api/data-transfer")
public class DataTransferController {
    private static final DateTimeFormatter EXPORT_TIMESTAMP_FORMATTER =
            DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss").withZone(ZoneOffset.UTC);
    private static final MediaType ZIP_MEDIA_TYPE = MediaType.parseMediaType("application/zip");
    private final DataTransferUseCase service;

    public DataTransferController(DataTransferUseCase service) {
        this.service = service;
    }

    @GetMapping("/export")
    public ResponseEntity<byte[]> exportData() throws IOException {
        byte[] bytes = service.exportZip();
        String filename = "omypic-backup-" + EXPORT_TIMESTAMP_FORMATTER.format(java.time.Instant.now()) + ".zip";
        return ResponseEntity.ok()
                .contentType(ZIP_MEDIA_TYPE)
                .header(HttpHeaders.CONTENT_DISPOSITION, ContentDisposition.attachment().filename(filename).build().toString())
                .body(bytes);
    }

    @PostMapping(value = "/import", consumes = MediaType.MULTIPART_FORM_DATA_VALUE)
    public DataTransferImportResponse importData(@RequestParam("file") MultipartFile file) throws IOException {
        DataTransferImportResult result = service.importZip(file.getBytes());
        return toResponse(result);
    }

    private DataTransferImportResponse toResponse(DataTransferImportResult result) {
        DataTransferImportResponse response = new DataTransferImportResponse();
        response.setImportedAt(result.importedAt());
        response.setQuestionGroupCount(result.questionGroupCount());
        response.setRulebookCount(result.rulebookCount());
        response.setWrongNoteCount(result.wrongNoteCount());
        response.setWrongNoteQueueSize(result.wrongNoteQueueSize());
        response.setRestartRequired(result.restartRequired());
        return response;
    }
}
