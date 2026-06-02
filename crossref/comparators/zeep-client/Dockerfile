FROM python:3.12-slim@sha256:090ba77e2958f6af52a5341f788b50b032dd4ca28377d2893dcf1ecbdfdfe203
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY client.py .
# Clear proxy environment variables that may be injected by the container runtime
# (e.g. OrbStack's proxyproxy.orb.internal). Internal compose-network traffic must
# never go through an external proxy.
ENV http_proxy="" https_proxy="" HTTP_PROXY="" HTTPS_PROXY="" all_proxy="" ALL_PROXY=""
ENTRYPOINT ["python", "client.py"]
