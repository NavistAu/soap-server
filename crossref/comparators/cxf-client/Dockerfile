# Multi-stage build: Maven build stage
FROM maven:3.9-eclipse-temurin-21@sha256:1bb51c5ed28b95aef2bc7b46bff6940da43747cdaf838ce4afc2081ce9403750 AS build
WORKDIR /build
COPY pom.xml .
# Pre-fetch dependencies
RUN mvn -B dependency:go-offline -q
COPY src ./src
RUN mvn -B package -DskipTests -q

# Runtime stage
FROM eclipse-temurin:21-jre@sha256:010e0a06bd4e0184dec58626afb3ba727b42c56c91b977e2f0a9e0837e0fa3fb
WORKDIR /app
COPY --from=build /build/target/cxf-interop-client.jar /app/cxf-interop-client.jar
ENTRYPOINT ["java", "-jar", "/app/cxf-interop-client.jar"]
