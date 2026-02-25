FROM ollama/ollama:latest

ARG OLLAMA_TEST_MODEL=qwen2.5:0.5b
ENV OLLAMA_HOST=127.0.0.1:11434

# Preload the model during image build so test runs do not pull at runtime.
RUN ollama serve > /tmp/ollama-build.log 2>&1 & \
    OLLAMA_PID=$! && \
    i=0 && \
    until ollama list >/dev/null 2>&1; do \
      i=$((i + 1)); \
      if [ "$i" -ge 120 ]; then \
        echo "ollama did not become ready during image build"; \
        cat /tmp/ollama-build.log; \
        kill "$OLLAMA_PID" || true; \
        wait "$OLLAMA_PID" || true; \
        exit 1; \
      fi; \
      sleep 1; \
    done && \
    ollama pull "${OLLAMA_TEST_MODEL}" && \
    kill "$OLLAMA_PID" && \
    wait "$OLLAMA_PID" || true
