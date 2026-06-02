# Multi-stage build: Maven build stage
FROM maven:3.9-eclipse-temurin-21 AS build
WORKDIR /build
COPY pom.xml .
# Pre-fetch dependencies
RUN mvn -B dependency:go-offline -q
COPY src ./src
RUN mvn -B package -DskipTests -q

# Runtime stage
FROM eclipse-temurin:21-jre
WORKDIR /app
COPY --from=build /build/target/cxf-interop-client.jar /app/cxf-interop-client.jar
ENTRYPOINT ["java", "-jar", "/app/cxf-interop-client.jar"]
