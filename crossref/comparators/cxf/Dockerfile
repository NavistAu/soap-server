FROM maven:3.9-eclipse-temurin-21 AS build
WORKDIR /src
COPY pom.xml .
RUN mvn -q dependency:go-offline
COPY src ./src
RUN mvn -q package

FROM eclipse-temurin:21-jre
WORKDIR /app
COPY --from=build /src/target/cxf-ref.jar /app/cxf-ref.jar
EXPOSE 8082
ENTRYPOINT ["java", "-jar", "/app/cxf-ref.jar"]
