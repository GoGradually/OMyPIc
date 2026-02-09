package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.application.rulebook.port.EmbeddingPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import org.apache.lucene.analysis.standard.StandardAnalyzer;
import org.apache.lucene.document.*;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.IndexWriterConfig;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.KnnVectorQuery;
import org.apache.lucene.search.ScoreDoc;
import org.apache.lucene.search.TopDocs;
import org.apache.lucene.store.Directory;
import org.apache.lucene.store.FSDirectory;
import org.springframework.stereotype.Component;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;

@Component
public class LuceneRulebookIndexAdapter implements RulebookIndexPort {
    private final Path indexPath;
    private final EmbeddingPort embeddingService;

    public LuceneRulebookIndexAdapter(DataDirProvider dataDirProvider, EmbeddingPort embeddingService) {
        this.indexPath = Path.of(dataDirProvider.getDataDir(), "indexes", "rulebooks");
        this.embeddingService = embeddingService;
        try {
            Files.createDirectories(indexPath);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to create index directory", e);
        }
    }

    @Override
    public void indexRulebookChunks(RulebookId rulebookId, String filename, List<String> chunks) throws IOException {
        Directory directory = FSDirectory.open(indexPath);
        IndexWriterConfig config = new IndexWriterConfig(new StandardAnalyzer());
        config.setOpenMode(IndexWriterConfig.OpenMode.CREATE_OR_APPEND);
        try (IndexWriter writer = new IndexWriter(directory, config)) {
            for (String chunk : chunks) {
                float[] vector = embeddingService.embed(chunk);
                Document doc = new Document();
                doc.add(new StringField("rulebookId", rulebookId.value(), Field.Store.YES));
                doc.add(new StringField("filename", filename, Field.Store.YES));
                doc.add(new StoredField("text", chunk));
                doc.add(new KnnVectorField("embedding", vector));
                writer.addDocument(doc);
            }
            writer.commit();
        }
    }

    @Override
    public List<RulebookContext> search(String query, int topK, Set<RulebookId> enabledRulebookIds) throws IOException {
        if (enabledRulebookIds.isEmpty()) {
            return List.of();
        }
        Directory directory = FSDirectory.open(indexPath);
        if (!DirectoryReader.indexExists(directory)) {
            return List.of();
        }
        Set<String> enabledIds = enabledRulebookIds.stream()
                .map(RulebookId::value)
                .collect(java.util.stream.Collectors.toSet());
        float[] queryVector = embeddingService.embed(query);
        try (DirectoryReader reader = DirectoryReader.open(directory)) {
            IndexSearcher searcher = new IndexSearcher(reader);
            KnnVectorQuery vectorQuery = new KnnVectorQuery("embedding", queryVector, topK * 3);
            TopDocs topDocs = searcher.search(vectorQuery, topK * 3);
            List<RulebookContext> results = new ArrayList<>();
            for (ScoreDoc scoreDoc : topDocs.scoreDocs) {
                Document doc = searcher.doc(scoreDoc.doc);
                String rulebookId = doc.get("rulebookId");
                if (!enabledIds.contains(rulebookId)) {
                    continue;
                }
                String filename = doc.get("filename");
                String text = doc.get("text");
                results.add(new RulebookContext(RulebookId.of(rulebookId), filename, text));
                if (results.size() >= topK) {
                    break;
                }
            }
            return results;
        }
    }
}
