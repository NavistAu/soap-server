FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY client.py .
# Clear proxy environment variables that may be injected by the container runtime
# (e.g. OrbStack's proxyproxy.orb.internal). Internal compose-network traffic must
# never go through an external proxy.
ENV http_proxy="" https_proxy="" HTTP_PROXY="" HTTPS_PROXY="" all_proxy="" ALL_PROXY=""
ENTRYPOINT ["python", "client.py"]
