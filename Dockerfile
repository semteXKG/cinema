FROM python:3.13-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt \
    && useradd -r -u 10001 ov \
    && mkdir -p /data \
    && chown ov /data
COPY app/ app/
USER ov
ENV DATA_DIR=/data PORT=8080
EXPOSE 8080
CMD ["python", "-m", "app.main"]
