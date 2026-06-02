# build stage — all Java compilation happens here; no Java on host
FROM maven:3.9-eclipse-temurin-21@sha256:1bb51c5ed28b95aef2bc7b46bff6940da43747cdaf838ce4afc2081ce9403750 AS build
WORKDIR /src
COPY pom.xml .
RUN mvn -q dependency:go-offline
COPY src ./src
RUN mvn -q package -DskipTests

# runtime stage — only the JRE + the shaded jar
FROM eclipse-temurin:21-jre@sha256:010e0a06bd4e0184dec58626afb3ba727b42c56c91b977e2f0a9e0837e0fa3fb
WORKDIR /app
COPY --from=build /src/target/oracle.jar /app/oracle.jar
EXPOSE 8081
ENTRYPOINT ["java", "-jar", "/app/oracle.jar"]
