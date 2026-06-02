# build stage — all Java compilation happens here; no Java on host
FROM maven:3.9-eclipse-temurin-21 AS build
WORKDIR /src
COPY pom.xml .
RUN mvn -q dependency:go-offline
COPY src ./src
RUN mvn -q package -DskipTests

# runtime stage — only the JRE + the shaded jar
FROM eclipse-temurin:21-jre
WORKDIR /app
COPY --from=build /src/target/oracle.jar /app/oracle.jar
EXPOSE 8081
ENTRYPOINT ["java", "-jar", "/app/oracle.jar"]
